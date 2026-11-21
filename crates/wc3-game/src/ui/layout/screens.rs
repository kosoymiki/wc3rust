//! Screen composition for the menu / glue screens (hud.md §4 + §5).
//!
//! `mobile_hud::layout_menu` is the shipped adapter in **framebuffer px**; this
//! module is the layer's own plan in **dp** and it answers the question the
//! adapter used to answer by feel: *does this screen fit the safe frame with
//! every label readable?*
//!
//! Two rules drive the numbers:
//!
//! 1. Containment. The panel is centred inside `Window::available()` (the
//!    sanitised safe frame), and every row/button is laid out inside the
//!    panel's content box. A row is never pushed past the content edge.
//! 2. Readability. A row is only planned when it can hold the smallest type
//!    token ([`TypeRole::Compact`]) on one line with the label inset. When the
//!    entries do not fit at that floor the plan **paginates** — it never shrinks
//!    the rows into unreadable strips the way a pure `height / count` solve
//!    does (the 320×240 defect: six 6.8 px lanes with clipped labels).
//!
//! The label ladder is the theme's type scale, not ad-hoc constants: a row
//! picks the largest role that fits its box, and when even `Compact` cannot fit
//! the *width* the text is ellipsised with `keep_chars` from
//! [`text_fit::fit`]. The plan reports `truncated`, so a caller can never
//! mistake an ellipsis for a full label.

use crate::ui::core::geometry::{Insets, Rect, Size};
use crate::ui::layout::engine::Window;
use crate::ui::layout::text_fit;
use crate::ui::theme::typography::{TypeRole, TypeStyle};
use crate::ui::theme::Theme;

/// Outer margin from the safe frame to the panel, in dp.
pub const MENU_MARGIN_DP: f32 = 16.0;
/// Padding inside the panel, in dp.
pub const CONTENT_PAD_DP: f32 = 12.0;
/// Gap between stacked rows, in dp.
pub const ROW_GAP_DP: f32 = 8.0;
/// Horizontal inset of a label inside its row, in dp.
pub const LABEL_PAD_DP: f32 = 6.0;
/// Widest menu panel, in dp (the retail glue panel is a narrow vertical plate).
pub const PANEL_MAX_W_DP: f32 = 420.0;
/// Tallest menu panel relative to its width: the plate stays a plate.
pub const PANEL_MAX_ASPECT: f32 = 1.60;
/// Bottom status band, in dp (message / progress text).
pub const STATUS_H_DP: f32 = 20.0;
/// Empty space between the title block and the stack, in dp.
pub const TITLE_GAP_DP: f32 = 8.0;
/// A row taller than this is wasted space; the panel shrinks instead.
pub const ROW_MAX_DP: f32 = 64.0;
/// Dialog footer buttons never get thinner than this, in dp.
pub const BUTTON_MIN_DP: f32 = 30.0;

/// Label ladder from the type scale, largest first. Every row starts at the
/// top and steps down only when the box cannot hold the role.
pub const LABEL_LADDER: [TypeRole; 5] = [
    TypeRole::Title,
    TypeRole::Heading,
    TypeRole::Body,
    TypeRole::Label,
    TypeRole::Compact,
];

/// One label box plus its resolved type and fit verdict.
#[derive(Clone, PartialEq, Debug)]
pub struct LabelPlan {
    /// Text to draw: the input, or its ellipsised prefix.
    pub text: String,
    pub role: TypeRole,
    /// Resolved size of the role in dp.
    pub size_dp: f32,
    /// Line box of that size in dp.
    pub line_dp: f32,
    pub rect: Rect,
    pub fit: text_fit::TextFit,
    /// The text did not fit whole and was ellipsised.
    pub truncated: bool,
}

impl LabelPlan {
    /// The whole label is inside its box with no ellipsis.
    pub fn fits(&self) -> bool {
        !self.truncated && self.fit.fits
    }

    /// The drawn text is inside the box (an ellipsis counts: nothing spills).
    /// An empty label is trivially contained.
    pub fn contained(&self) -> bool {
        self.text.is_empty() || (self.fit.fits_one_line && !self.text.is_empty())
    }
}

/// A planned menu (or any stacked glue screen) for one page.
#[derive(Clone, PartialEq, Debug)]
pub struct MenuPlan {
    /// The logical viewport in dp.
    pub viewport: Rect,
    /// The sanitised safe frame in dp (`Window::available`).
    pub safe: Rect,
    pub panel: Rect,
    pub content: Rect,
    pub title_band: Rect,
    pub title: LabelPlan,
    pub subtitle: LabelPlan,
    /// Message / progress band at the bottom (empty when there is no room).
    pub status: Rect,
    /// Visible entry rows for this page, top to bottom.
    pub rows: Vec<LabelPlan>,
    /// Footer navigation buttons (BACK / NEXT), spread across the width.
    pub nav: Vec<LabelPlan>,
    pub row_h: f32,
    pub page: usize,
    pub page_count: usize,
    pub per_page: usize,
    /// Total entries the screen holds (not just this page).
    pub total: usize,
    /// Even the smallest token could not hold a row: the screen does not fit.
    pub readable: bool,
}

impl MenuPlan {
    /// Every label on this page is drawn whole (no ellipsis).
    pub fn labels_fit(&self) -> bool {
        self.rows.iter().all(LabelPlan::fits) && self.nav.iter().all(LabelPlan::fits)
    }

    /// Every drawn label stays inside its box (whole or ellipsised).
    pub fn contained(&self) -> bool {
        self.rows.iter().all(LabelPlan::contained) && self.nav.iter().all(LabelPlan::contained)
    }
}

/// A planned dialog (game menu / victory / defeat).
#[derive(Clone, PartialEq, Debug)]
pub struct DialogPlan {
    pub viewport: Rect,
    pub safe: Rect,
    pub panel: Rect,
    pub content: Rect,
    /// Title cartouche at the top of the content.
    pub banner: LabelPlan,
    /// Body area under the banner (quest text, stats): empty for the caller.
    pub body: Rect,
    /// Footer buttons, left to right.
    pub buttons: Vec<LabelPlan>,
    pub readable: bool,
}

/// Fit `text` into `rect` using `ladder` (largest first).
///
/// The returned plan always reports the truth: `truncated` when the string was
/// shortened, and `fit` recomputed against the shortened string, so
/// `advance(plan.text) <= rect.w` holds for a positive-width box.
pub fn fit_label(text: &str, rect: Rect, theme: &Theme, ladder: &[TypeRole]) -> LabelPlan {
    let style_of = |role: TypeRole| theme.type_scale.style(role);
    for role in ladder {
        let style = style_of(*role);
        let fit = text_fit::fit(text, style.size_dp, rect.w, rect.h, false);
        if fit.fits {
            return LabelPlan {
                text: text.to_string(),
                role: *role,
                size_dp: style.size_dp,
                line_dp: style.line_dp,
                rect,
                fit,
                truncated: false,
            };
        }
    }
    // Nothing fits: keep the smallest role, ellipsise to the measured budget.
    let style = style_of(*ladder.last().expect("ladder is non-empty"));
    let mut fit = text_fit::fit(text, style.size_dp, rect.w, rect.h, false);
    let mut shown = text.to_string();
    let mut truncated = false;
    if !fit.fits_one_line {
        // Bounded descent: every step removes one glyph, so this terminates at
        // the bare ellipsis. (`keep_chars` counts the ellipsis already.)
        let mut budget = fit.keep_chars.max(2) as usize;
        loop {
            let candidate = ellipsise(text, budget);
            let candidate_fit = text_fit::fit(&candidate, style.size_dp, rect.w, rect.h, false);
            shown = candidate;
            fit = candidate_fit;
            truncated = shown != text;
            if fit.fits_one_line || budget <= 1 {
                break;
            }
            budget -= 1;
        }
    }
    LabelPlan {
        text: shown,
        role: *ladder.last().expect("ladder is non-empty"),
        size_dp: style.size_dp,
        line_dp: style.line_dp,
        rect,
        fit,
        truncated: truncated || !fit.fits_one_line,
    }
}

/// Ellipsised prefix that fits `budget` characters (ellipsis included).
fn ellipsise(text: &str, budget: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= budget {
        return text.to_string();
    }
    match budget {
        0 => String::new(),
        1 => "…".to_string(),
        _ => {
            let mut out: String = chars.iter().take(budget - 1).collect();
            out.push('…');
            out
        }
    }
}

/// Height a row needs to hold one line of `role` with the label inset, in dp.
pub fn row_floor(theme: &Theme, role: TypeRole) -> f32 {
    let style = theme.type_scale.style(role);
    (style.line_dp + 2.0 * LABEL_PAD_DP).max(1.0)
}

/// Rows of at least `floor` that fit `avail` with `gap` between them.
pub fn rows_that_fit(avail: f32, gap: f32, floor: f32) -> usize {
    if !avail.is_finite() || !floor.is_finite() || avail < floor {
        return 0;
    }
    let gap = gap.max(0.0);
    (1.0 + ((avail - floor) / (floor + gap)).floor()).max(0.0) as usize
}

/// Logical viewport as a rect at the origin.
fn viewport_rect(window: &Window) -> Rect {
    let dp = window.dp();
    Rect::new(0.0, 0.0, dp.w.max(1.0), dp.h.max(1.0))
}

/// Centred panel of `w` × `h` inside `safe`.
fn centred_in(safe: Rect, w: f32, h: f32) -> Rect {
    let w = w.clamp(1.0, safe.w.max(1.0));
    let h = h.clamp(1.0, safe.h.max(1.0));
    Rect::new(
        safe.x + (safe.w - w) * 0.5,
        safe.y + (safe.h - h) * 0.5,
        w,
        h,
    )
}

/// Split `area` into `count` rows separated by `gap`, flush with both edges.
fn split_rows(area: Rect, count: usize, gap: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let gap = gap.max(0.0);
    let row_h = ((area.h - gap * count.saturating_sub(1) as f32) / count as f32).max(0.0);
    (0..count)
        .map(|index| {
            Rect::new(
                area.x,
                area.y + index as f32 * (row_h + gap),
                area.w,
                row_h,
            )
        })
        .collect()
}

/// Shared panel/title geometry for menus and dialogs.
struct PanelFrame {
    safe: Rect,
    panel: Rect,
    content: Rect,
    title_band: Rect,
    subtitle: Rect,
    stack_top: f32,
    stack_bottom: f32,
    status: Rect,
    panel_h_max: f32,
}

fn panel_frame(window: &Window, theme: &Theme, row_floor_dp: f32, with_status: bool) -> PanelFrame {
    let safe = window.available();
    let margin = MENU_MARGIN_DP.max(0.0);
    let pad = CONTENT_PAD_DP.max(0.0);
    let panel_w = (safe.w - 2.0 * margin)
        .min(PANEL_MAX_W_DP)
        .max(1.0);
    let panel_h_max = (safe.h - 2.0 * margin).max(1.0);
    let title_style = theme.type_scale.style(TypeRole::Title);
    let subtitle_style = theme.type_scale.style(TypeRole::Label);
    let title_band_h = title_style.line_dp + 2.0 * LABEL_PAD_DP;
    // A short frame keeps the rows: the subtitle and the status line are the
    // first things to go, never the labels (320x240 had one entry per page
    // while a 22 dp subtitle and a 26 dp status band filled the stack).
    let short_frame = safe.h < 420.0;
    let subtitle_h = if short_frame { 0.0 } else { subtitle_style.line_dp + LABEL_PAD_DP };
    let with_status = with_status && safe.h >= 320.0;
    let status_h = if with_status {
        theme.type_scale.style(TypeRole::Compact).line_dp + 2.0 * LABEL_PAD_DP
    } else {
        0.0
    };

    let content_h_at_max = panel_h_max - 2.0 * pad;
    let stack_top = title_band_h + subtitle_h + TITLE_GAP_DP;
    let stack_bottom_inset = status_h + if with_status { TITLE_GAP_DP } else { 0.0 };
    let avail_at_max = (content_h_at_max - stack_top - stack_bottom_inset).max(0.0);
    let per_page = rows_that_fit(avail_at_max, ROW_GAP_DP, row_floor_dp).max(1);
    let needed_h = 2.0 * pad
        + stack_top
        + per_page as f32 * row_floor_dp
        + ROW_GAP_DP * per_page.saturating_sub(1) as f32
        + stack_bottom_inset;
    let panel_h = panel_h_max
        .min(needed_h.max(row_floor_dp + 2.0 * pad + stack_top))
        .min(panel_w * PANEL_MAX_ASPECT)
        .max(1.0);
    let panel = centred_in(safe, panel_w, panel_h);
    let content = panel.inset(pad, pad);
    let title_band = Rect::new(
        content.x,
        content.y,
        content.w,
        title_band_h.min(content.h.max(1.0)),
    );
    let subtitle = Rect::new(
        content.x,
        title_band.bottom(),
        content.w,
        subtitle_h.min((content.bottom() - title_band.bottom()).max(0.0)),
    );
    let status = if with_status && content.h > stack_top + row_floor_dp {
        Rect::new(content.x, content.bottom() - status_h, content.w, status_h)
    } else {
        Rect::ZERO
    };
    let stack_top_abs = subtitle.bottom() + TITLE_GAP_DP;
    let stack_bottom_abs = if status.h > 0.0 {
        status.y - TITLE_GAP_DP
    } else {
        content.bottom()
    };
    PanelFrame {
        safe,
        panel,
        content,
        title_band,
        subtitle,
        stack_top: stack_top_abs,
        stack_bottom: stack_bottom_abs.max(stack_top_abs),
        status,
        panel_h_max: content_h_at_max,
    }
}

/// Plan one page of a stacked glue screen (main menu, campaign list, options).
///
/// `entries` are the labels of the interactive rows in draw order; `nav` are
/// the footer buttons (BACK / NEXT). `page` is clamped into
/// `0..page_count`. A screen that does not fit one page is paginated rather
/// than squeezed: `page_count > 1` tells the caller to draw its counter.
pub fn plan_menu(
    window: &Window,
    theme: &Theme,
    title: &str,
    subtitle: &str,
    entries: &[&str],
    nav: &[&str],
    page: usize,
) -> MenuPlan {
    let floor = row_floor(theme, TypeRole::Compact);
    let frame = panel_frame(window, theme, floor, true);
    let total = entries.len();
    let nav_count = nav.len();

    // The nav row shares the stack budget, so page size is solved on the
    // entries only after the footer is reserved.
    let nav_h = if nav_count > 0 { floor } else { 0.0 };
    let nav_gap = if nav_count > 0 { ROW_GAP_DP } else { 0.0 };
    let avail = (frame.stack_bottom - frame.stack_top - nav_h - nav_gap).max(1.0);
    let per_page = rows_that_fit(avail, ROW_GAP_DP, floor).max(1).min(total.max(1));
    let page_count = if total == 0 {
        1
    } else {
        total.div_ceil(per_page)
    };
    let page = page.min(page_count.saturating_sub(1));
    let start = page * per_page;
    let end = (start + per_page).min(total);

    // Rows take all the room the page has, up to the cap; the reported row_h is
    // the actual lane height, never below the readable floor.
    let rows_h_avail = avail.max(floor);
    let visible = end.saturating_sub(start);
    let row_h = if visible == 0 {
        0.0
    } else {
        (((rows_h_avail - ROW_GAP_DP * visible.saturating_sub(1) as f32) / visible as f32)
            .max(floor)
            .min(ROW_MAX_DP))
        .max(1.0)
    };
    let rows_area = Rect::new(
        frame.content.x,
        frame.stack_top,
        frame.content.w,
        (row_h * visible as f32 + ROW_GAP_DP * visible.saturating_sub(1) as f32)
            .max(0.0)
            .min((frame.stack_bottom - frame.stack_top).max(0.0)),
    );
    let row_rects = split_rows(rows_area, visible, ROW_GAP_DP);

    let mut rows = Vec::with_capacity(visible);
    for (index, rect) in row_rects.iter().enumerate() {
        let label_rect = rect.inset(LABEL_PAD_DP, 0.0);
        rows.push(fit_label(entries[start + index], label_rect, theme, &LABEL_LADDER));
    }

    let mut navs = Vec::with_capacity(nav_count);
    if nav_count > 0 {
        let nav_area = Rect::new(
            frame.content.x,
            (frame.stack_bottom - nav_h).max(frame.stack_top),
            frame.content.w,
            nav_h.max(1.0),
        );
        let gap = ROW_GAP_DP;
        let cell_w = ((nav_area.w - gap * nav_count.saturating_sub(1) as f32) / nav_count as f32)
            .max(1.0);
        for (index, label) in nav.iter().enumerate() {
            let rect = Rect::new(nav_area.x + index as f32 * (cell_w + gap), nav_area.y, cell_w, nav_area.h);
            navs.push(fit_label(label, rect.inset(LABEL_PAD_DP, 0.0), theme, &LABEL_LADDER));
        }
    }

    let title_plan = fit_label(title, frame.title_band.inset(LABEL_PAD_DP, 0.0), theme, &LABEL_LADDER);
    let subtitle_plan = fit_label(
        subtitle,
        frame.subtitle.inset(LABEL_PAD_DP, 0.0),
        theme,
        &[TypeRole::Label, TypeRole::Compact],
    );

    let compact_line_dp = theme.type_scale.style(TypeRole::Compact).line_dp;
    let readable = (visible > 0 || total == 0)
        && rows.iter().all(LabelPlan::contained)
        && navs.iter().all(LabelPlan::contained)
        && title_plan.contained()
        && (frame.subtitle.h < compact_line_dp || subtitle_plan.contained())
        && (visible == 0 || row_h + 0.5 >= floor);

    MenuPlan {
        viewport: viewport_rect(window),
        safe: frame.safe,
        panel: frame.panel,
        content: frame.content,
        title_band: frame.title_band,
        title: title_plan,
        subtitle: subtitle_plan,
        status: frame.status,
        rows,
        nav: navs,
        row_h,
        page,
        page_count,
        per_page,
        total,
        readable,
    }
}

/// Plan a dialog: title cartouche, body area and a footer button row.
pub fn plan_dialog(
    window: &Window,
    theme: &Theme,
    title: &str,
    buttons: &[&str],
) -> DialogPlan {
    let floor = row_floor(theme, TypeRole::Compact).max(BUTTON_MIN_DP);
    let frame = panel_frame(window, theme, floor, false);
    let title_plan = fit_label(title, frame.title_band.inset(LABEL_PAD_DP, 0.0), theme, &LABEL_LADDER);

    let count = buttons.len();
    let (columns, button_rows) = if count == 0 {
        (0, 0)
    } else if count <= 2 {
        (count, 1)
    } else {
        (2, count.div_ceil(2))
    };
    let gap = ROW_GAP_DP;
    let buttons_h = if count == 0 {
        0.0
    } else {
        (button_rows as f32 * floor + gap * button_rows.saturating_sub(1) as f32)
            .min((frame.content.bottom() - frame.subtitle.bottom()).max(1.0))
    };
    let footer_y = (frame.content.bottom() - buttons_h).max(frame.subtitle.bottom() + TITLE_GAP_DP);
    let body = Rect::new(
        frame.content.x,
        frame.subtitle.bottom() + TITLE_GAP_DP,
        frame.content.w,
        (footer_y - frame.subtitle.bottom() - TITLE_GAP_DP).max(0.0),
    );

    let mut plans = Vec::with_capacity(count);
    if count > 0 {
        let cell_h = ((buttons_h - gap * button_rows.saturating_sub(1) as f32)
            / button_rows.max(1) as f32)
            .max(1.0);
        let cell_w = ((frame.content.w - gap * columns.saturating_sub(1) as f32)
            / columns.max(1) as f32)
            .max(1.0);
        for index in 0..count {
            let row = index / columns.max(1);
            let col = index % columns.max(1);
            let in_row = (count - row * columns).min(columns);
            let row_w = cell_w * in_row as f32 + gap * in_row.saturating_sub(1) as f32;
            let x0 = frame.content.x + (frame.content.w - row_w) * 0.5;
            let rect = Rect::new(x0 + col as f32 * (cell_w + gap), footer_y + row as f32 * (cell_h + gap), cell_w, cell_h);
            plans.push(fit_label(buttons[index], rect.inset(LABEL_PAD_DP, 0.0), theme, &LABEL_LADDER));
        }
    }

    let readable = title_plan.fits()
        && plans.iter().all(LabelPlan::fits)
        && (count == 0 || buttons_h + 0.5 >= floor);
    DialogPlan {
        viewport: viewport_rect(window),
        safe: frame.safe,
        panel: frame.panel,
        content: frame.content,
        banner: title_plan,
        body,
        buttons: plans,
        readable,
    }
}

/// The five viewport classes the screens are proved on, as `(label, window)`.
///
/// Densities and insets are the platform values the shipped example records
/// (`mobile_hud` REFERENCE_HEIGHT = 1080 px ⇒ the phone class is density 3).
pub fn shipped_windows() -> Vec<(&'static str, Window)> {
    let matrix: [(&'static str, (u32, u32), f32, [f32; 4]); 5] = [
        ("device-landscape", (2414, 1080), 3.0, [210.0, 120.0, 90.0, 96.0]),
        ("device-portrait", (1080, 2414), 3.0, [24.0, 96.0, 24.0, 64.0]),
        ("desktop-720p", (1280, 720), 1.0, [0.0; 4]),
        ("wide-2340", (2340, 1080), 2.75, [120.0, 80.0, 48.0, 48.0]),
        ("compact-320x240", (320, 240), 1.0, [0.0; 4]),
    ];
    matrix
        .iter()
        .map(|(label, (w, h), density, insets)| {
            let window = Window::from_pixels(
                Size::new(*w as f32, *h as f32),
                *density,
                Insets::new(insets[0], insets[1], insets[2], insets[3]),
            );
            (*label, window)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six main-menu entries of the shipped glue screen.
    pub const MAIN_ENTRIES: [&str; 6] = [
        "CAMPAIGN",
        "CUSTOM GAME",
        "SINGLE PLAYER",
        "LOAD GAME",
        "OPTIONS",
        "QUIT",
    ];

    fn theme() -> Theme {
        Theme::classic_fantasy()
    }

    fn contains(inner: Rect, outer: Rect) -> bool {
        let tol = 1e-3;
        inner.x >= outer.x - tol
            && inner.y >= outer.y - tol
            && inner.right() <= outer.right() + tol
            && inner.bottom() <= outer.bottom() + tol
            && inner.w > 0.0
            && inner.h > 0.0
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.x < b.right() - 1e-3
            && b.x < a.right() - 1e-3
            && a.y < b.bottom() - 1e-3
            && b.y < a.bottom() - 1e-3
    }

    fn finite(rect: Rect) -> bool {
        [rect.x, rect.y, rect.w, rect.h]
            .iter()
            .all(|value| value.is_finite())
    }

    #[test]
    fn ui_screens_menu_fits_the_five_shipped_viewports() {
        let theme = theme();
        for (label, window) in shipped_windows() {
            let plan = plan_menu(
                &window,
                &theme,
                "WARCRAFT III",
                "REIGN OF CHAOS",
                &MAIN_ENTRIES,
                &["BACK"],
                0,
            );
            assert!(finite(plan.panel) && finite(plan.content), "{label}: finite");
            assert!(contains(plan.panel, plan.viewport), "{label}: panel in viewport");
            assert!(contains(plan.panel, plan.safe), "{label}: panel in safe frame");
            assert!(contains(plan.content, plan.panel), "{label}: content in panel");
            assert!(
                contains(plan.title_band, plan.content),
                "{label}: title band in content"
            );
            assert!(plan.title.fits(), "{label}: title {:?}", plan.title);
            if plan.subtitle.rect.h > 0.0 {
                assert!(plan.subtitle.fits(), "{label}: subtitle {:?}", plan.subtitle);
            }
            assert!(plan.readable, "{label}: plan is readable");
            assert!(
                plan.labels_fit(),
                "{label}: main-menu labels are drawn whole (no ellipsis)"
            );
            assert!(!plan.rows.is_empty(), "{label}: rows");
            for (index, row) in plan.rows.iter().enumerate() {
                assert!(
                    contains(row.rect, plan.content),
                    "{label}: row {index} {:?} outside content {:?}",
                    row.rect,
                    plan.content
                );
                assert!(row.fits(), "{label}: row {index} label {:?}", row);
                assert!(
                    row.size_dp >= 11.0 - 1e-3,
                    "{label}: row {index} below the compact token"
                );
            }
            for (i, a) in plan.rows.iter().enumerate() {
                for (j, b) in plan.rows.iter().enumerate().skip(i + 1) {
                    assert!(!overlap(a.rect, b.rect), "{label}: rows {i},{j} overlap");
                }
            }
            for nav in &plan.nav {
                assert!(contains(nav.rect, plan.content), "{label}: nav outside");
                assert!(nav.fits(), "{label}: nav label {:?}", nav);
            }
            assert!(
                plan.per_page * plan.page_count >= plan.total,
                "{label}: pagination covers every entry ({} x {} < {})",
                plan.per_page,
                plan.page_count,
                plan.total
            );
        }
    }

    #[test]
    fn ui_screens_dialog_fits_banner_and_buttons() {
        let theme = theme();
        for (label, window) in shipped_windows() {
            for count in 1..=3usize {
                let buttons: Vec<&str> = ["RESUME", "SAVE GAME", "QUIT TO MAIN MENU"]
                    .into_iter()
                    .take(count)
                    .collect();
                let plan = plan_dialog(&window, &theme, "GAME PAUSED", &buttons);
                assert!(contains(plan.panel, plan.viewport), "{label}: panel in viewport");
                assert!(contains(plan.panel, plan.safe), "{label}: panel in safe frame");
                assert!(contains(plan.content, plan.panel), "{label}: content in panel");
                assert!(plan.banner.fits(), "{label}/{count}: banner {:?}", plan.banner);
                assert_eq!(plan.buttons.len(), count, "{label}/{count}: every button");
                for button in &plan.buttons {
                    assert!(
                        contains(button.rect, plan.content),
                        "{label}/{count}: button {:?} outside content",
                        button.rect
                    );
                    assert!(button.fits(), "{label}/{count}: button label {:?}", button);
                }
                for (i, a) in plan.buttons.iter().enumerate() {
                    for (j, b) in plan.buttons.iter().enumerate().skip(i + 1) {
                        assert!(
                            !overlap(a.rect, b.rect),
                            "{label}/{count}: buttons {i},{j} overlap"
                        );
                    }
                }
                assert!(plan.readable, "{label}/{count}: readable dialog");
            }
        }
    }

    #[test]
    fn ui_screens_compact_viewport_paginates_instead_of_shrinking_rows() {
        let theme = theme();
        let window = shipped_windows()
            .into_iter()
            .find(|(label, _)| *label == "compact-320x240")
            .map(|(_, window)| window)
            .expect("compact window");
        let first = plan_menu(&window, &theme, "WARCRAFT III", "REIGN OF CHAOS", &MAIN_ENTRIES, &["BACK"], 0);
        assert!(
            first.page_count >= 2,
            "six entries cannot fit at the readable floor on 320x240: {first:#?}"
        );
        assert!(first.row_h >= row_floor(&theme, TypeRole::Compact) - 0.5);
        let mut seen = first.rows.len();
        for page in 1..first.page_count {
            let next = plan_menu(&window, &theme, "WARCRAFT III", "REIGN OF CHAOS", &MAIN_ENTRIES, &["BACK"], page);
            assert_eq!(next.page, page);
            assert!(next.readable, "page {page} readable");
            for row in &next.rows {
                assert!(row.fits(), "page {page} label {:?}", row);
            }
            seen += next.rows.len();
        }
        assert_eq!(seen, MAIN_ENTRIES.len(), "every entry is reachable exactly once");
        // A requested page past the end clamps instead of panicking or empty.
        let clamped = plan_menu(&window, &theme, "WARCRAFT III", "REIGN OF CHAOS", &MAIN_ENTRIES, &["BACK"], 99);
        assert_eq!(clamped.page, first.page_count - 1);
        assert!(!clamped.rows.is_empty());
    }

    #[test]
    fn ui_screens_safe_area_and_degenerate_inputs_stay_finite() {
        let theme = theme();
        // A notch that eats a fifth of every edge.
        let notch = Window::from_pixels(
            Size::new(2414.0, 1080.0),
            3.0,
            Insets::new(480.0, 240.0, 480.0, 240.0),
        );
        let plan = plan_menu(&notch, &theme, "MAIN", "SUB", &MAIN_ENTRIES, &["BACK"], 0);
        assert!(contains(plan.panel, notch.available()), "panel in the notch safe frame");
        for row in &plan.rows {
            assert!(contains(row.rect, notch.available()), "row in safe frame");
        }

        // Absurd insets are clamped by the solver (<= 40% per edge), never NaN.
        let absurd = Window::from_pixels(
            Size::new(320.0, 240.0),
            1.0,
            Insets::new(10_000.0, 10_000.0, 10_000.0, 10_000.0),
        );
        let plan = plan_menu(&absurd, &theme, "MAIN", "SUB", &MAIN_ENTRIES, &["BACK"], 0);
        assert!(finite(plan.panel) && plan.panel.w >= 1.0 && plan.panel.h >= 1.0);
        assert!(finite(plan.content));
        assert!(contains(plan.panel, absurd.available()));

        // A 1x1 window must not divide by zero or paint outside.
        let tiny = Window::new(Size::new(1.0, 1.0), 1.0);
        let plan = plan_menu(&tiny, &theme, "T", "S", &["A"], &[], 0);
        assert!(finite(plan.panel) && finite(plan.content));
        assert!(!plan.readable, "1x1 cannot hold a label");
        let dialog = plan_dialog(&tiny, &theme, "T", &["OK"]);
        assert!(finite(dialog.panel) && finite(dialog.content));
    }

    #[test]
    fn ui_screens_plan_is_deterministic() {
        let theme = theme();
        for (_, window) in shipped_windows() {
            let a = plan_menu(&window, &theme, "TITLE", "SUB", &MAIN_ENTRIES, &["BACK", "NEXT"], 1);
            let b = plan_menu(&window, &theme, "TITLE", "SUB", &MAIN_ENTRIES, &["BACK", "NEXT"], 1);
            assert_eq!(a, b, "the same input plans the same screen");
        }
    }

    #[test]
    fn ui_screens_ellipsis_is_measured_not_clipped() {
        let theme = theme();
        let rect = Rect::new(0.0, 0.0, 60.0, 20.0);
        let plan = fit_label("A VERY LONG CAMPAIGN CHAPTER NAME", rect, &theme, &LABEL_LADDER);
        assert!(plan.truncated, "the string cannot fit 60 dp");
        assert!(plan.text.ends_with('…'), "ellipsis marker: {:?}", plan.text);
        assert!(
            plan.fit.fits_one_line,
            "the ellipsised string fits: {:?} {:?}",
            plan.text, plan.fit
        );
        assert!(text_fit::advance(&plan.text, plan.size_dp) <= rect.w + 1.0);
        // A short label is never touched.
        let short = fit_label("QUIT", rect, &theme, &LABEL_LADDER);
        assert!(!short.truncated && short.fits());
        // Degenerate boxes terminate at the bare ellipsis (regression: the first
        // fallback popped the ellipsis and pushed it back, looping forever).
        for (w, h) in [(1.0_f32, 1.0_f32), (2.0, 6.0), (0.0, 0.0)] {
            let tiny = Rect::new(0.0, 0.0, w, h);
            let plan = fit_label("A VERY LONG CAMPAIGN CHAPTER NAME", tiny, &theme, &LABEL_LADDER);
            assert!(plan.truncated, "{w}x{h}: cannot fit");
            assert!(
                plan.text == "…" || plan.text.is_empty() || plan.fit.fits_one_line,
                "{w}x{h}: bounded fallback, got {:?}",
                plan.text
            );
        }
    }
}
