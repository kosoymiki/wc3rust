//! Menu / dialog panels and their buttons.
//!
//! Region: the main menu, the skirmish setup panel, victory/defeat, the game
//! menu and the quest-log list. WC3 look: stone plates with a gold bevel, red
//! banners baked into the frame art, and rows of wide buttons with a pressed
//! state.
//!
//! Contract (`hudskin/mod.rs`, read-only): this module decides *which* atlas
//! role and *where*; nine-patch geometry and UVs live in `Skin::nine_scaled`
//! (`Skin::nine` = scale 1). [`nine_scaled`] here is a thin wrapper that keeps
//! the bevel inside the target: when the scaled border no longer fits the rect
//! it reduces the *art* scale uniformly, so a short quest row shows a thin frame
//! instead of squeezing the whole plate sheet into a strip.
//!
//! Sizing rule that keeps the mock atlas clean: `dialog_wide` and `list_panel`
//! have row guides baked into their centres, so the composition helpers size
//! those panels as `atlas_size * scale` (a uniform scale of the art, no
//! smearing of the guides). `dialog_panel` has a clean centre and is free to
//! take any rect; the nine-patch then stretches only its centre.
//!
//! Nothing here draws text: every `draw_*` returns the rect the caller puts the
//! label into.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Primary atlas role of this region (the dialog/victory panel).
pub const ROLE: &str = "dialog_panel";
/// Main menu / skirmish setup plate (4 baked row guides).
pub const ROLE_MENU: &str = "dialog_wide";
/// Game menu, victory and defeat plate (clean centre, red side banners).
pub const ROLE_DIALOG: &str = "dialog_panel";
/// Quest-log list frame.
pub const ROLE_QUEST: &str = "list_panel";
/// Title plate (the skirmish header / dialog title cartouche).
pub const ROLE_BANNER: &str = "wide_panel";
/// Button at rest.
pub const ROLE_BUTTON: &str = "button_normal";
/// Button held down.
pub const ROLE_BUTTON_PRESSED: &str = "menu_button_pressed";

/// Which plate a menu-ish surface is drawn on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    /// Main menu and skirmish setup: the wide four-lane option plate.
    Menu,
    /// Game menu, victory and defeat: the square framed plate.
    Dialog,
    /// Quest log: the framed list.
    Quest,
}

impl PanelKind {
    pub const ALL: [PanelKind; 3] = [PanelKind::Menu, PanelKind::Dialog, PanelKind::Quest];

    /// Atlas role this kind is drawn with.
    pub fn role(self) -> &'static str {
        match self {
            PanelKind::Menu => ROLE_MENU,
            PanelKind::Dialog => ROLE_DIALOG,
            PanelKind::Quest => ROLE_QUEST,
        }
    }

    /// Base tint. The art already carries the gold bevel and the red banners;
    /// the tint only sets how far the plate sits back from the screen content.
    pub fn tint(self) -> [f32; 4] {
        match self {
            PanelKind::Menu => PANEL_TINT_MENU,
            PanelKind::Dialog => PANEL_TINT_DIALOG,
            PanelKind::Quest => PANEL_TINT_QUEST,
        }
    }
}

/// Below this the scale is treated as a degenerate zero.
pub const MIN_SCALE: f32 = 0.01;
/// Smallest art scale `Skin::nine_scaled` honours; borders below it stop
/// shrinking so a degenerate panel still shows its bevel.
pub const MIN_ART_SCALE: f32 = 0.25;
/// Widest scale accepted by the fit helpers (a mock atlas upscaled further
/// than this reads as mush).
pub const MAX_PANEL_SCALE: f32 = 8.0;
/// Gutter between the measured bevel and the content box, in logical px.
pub const CONTENT_PAD: f32 = 8.0;
/// Gutter between a button bevel and its label box, in logical px.
pub const BUTTON_PAD: f32 = 10.0;
/// A button never loses more than this fraction of its side to the bevel when
/// the label box is computed (`button_normal` is a two-variant sheet, so its
/// measured vertical insets exceed a single plate).
pub const LABEL_INSET_FRAC: f32 = 0.18;
/// Authored button height (`HUD_REDRAW_TZ.md` §5 item 19: menu_button 256x64).
pub const BUTTON_H: f32 = 64.0;
/// Gap between stacked menu buttons, in logical px.
pub const BUTTON_GAP: f32 = 8.0;
/// Preferred quest row height; rows shrink to fit the list (`row_pitch`).
pub const ROW_H: f32 = 64.0;
/// Gap between quest rows, in logical px.
pub const ROW_GAP: f32 = 4.0;
/// Gap between menu lanes. Small on purpose: `dialog_wide` has the row
/// separators baked into its art, the gap only keeps the button bevels apart.
pub const MENU_ROW_GAP: f32 = 2.0;
/// Authored banner height (`wide_panel` is 216x118).
pub const BANNER_H: f32 = 118.0;
/// Press shrink: 0.96 of the rect, i.e. 2% off each side (`HUD_REDRAW_TZ` §8).
pub const PRESS_SHRINK: f32 = 0.02;
/// Disabled buttons are tinted down by 35%.
pub const DISABLED_DIM: f32 = 0.65;
/// A nine-patch cell never collapses below this many screen px.
pub const MIN_CELL: f32 = 1.0;
/// Banner height cap as a fraction of the dialog content.
pub const BANNER_MAX_FRAC: f32 = 0.28;
/// Smallest quest row that is still a row.
pub const MIN_ROW: f32 = 1.0;

pub const PANEL_TINT_MENU: [f32; 4] = [1.0, 0.99, 0.97, 1.0];
pub const PANEL_TINT_DIALOG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const PANEL_TINT_QUEST: [f32; 4] = [0.94, 0.92, 0.88, 1.0];
pub const BANNER_TINT: [f32; 4] = [1.0, 0.93, 0.90, 1.0];
pub const BUTTON_TINT: [f32; 4] = [1.0, 0.98, 0.93, 1.0];
/// 8% darker than [`BUTTON_TINT`] (`HUD_REDRAW_TZ` §8 press darkening).
pub const BUTTON_PRESSED_TINT: [f32; 4] = [0.92, 0.90, 0.86, 1.0];
pub const ROW_TINT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Alternate quest row: visibly darker so the list reads as rows.
pub const ROW_TINT_ALT: [f32; 4] = [0.84, 0.82, 0.79, 1.0];

/// Size of `role` in atlas px.
pub fn role_size(skin: &Skin, role: &str) -> Option<(f32, f32)> {
    Skin::role_index(role).map(|index| (skin.rects[index][2], skin.rects[index][3]))
}

/// Measured border of `role` in screen px at `scale` (`[left, top, right, bottom]`).
///
/// The atlas insets are authored in atlas pixels; a DPI scale multiplies the
/// *destination* thickness only, so the corner texels never change.
pub fn border(skin: &Skin, role: &str, scale: f32) -> [f32; 4] {
    let Some(index) = Skin::role_index(role) else {
        return [0.0; 4];
    };
    let s = scale.max(MIN_ART_SCALE);
    let insets = skin.insets[index];
    [
        insets[0] * s,
        insets[1] * s,
        insets[2] * s,
        insets[3] * s,
    ]
}

/// Nine-patch `role` into `rect` at `scale`, with the bevel kept inside the rect.
///
/// Delegates the geometry and the UVs to [`Skin::nine_scaled`]; the only local
/// decision is the *effective* art scale: when `insets * scale` no longer leaves
/// a [`MIN_CELL`] centre, the art scale is reduced uniformly (`min` over both
/// axes) so the patch degrades to a thinner frame. Without that, a quest row
/// shorter than the list bevel makes the shared nine-patch give up and squeeze
/// the whole 218x287 sheet into the row. Returns `false` when the role is absent.
pub fn nine_scaled(
    skin: &Skin,
    batch: &mut UiBatch,
    role: &str,
    rect: Rect,
    scale: f32,
    tint: [f32; 4],
) -> bool {
    let Some(index) = Skin::role_index(role) else {
        return false;
    };
    let [_, _, aw, ah] = skin.rects[index];
    if aw <= 0.0 || ah <= 0.0 {
        return false;
    }
    if rect.w < 2.0 * MIN_CELL || rect.h < 2.0 * MIN_CELL {
        // Too small for cells at all: one quad, never a hole in the layout.
        return skin.stretch(batch, role, rect, tint);
    }
    let [il, it, ir, ib] = skin.insets[index];
    let s = scale.max(MIN_ART_SCALE);
    // `Skin::nine_scaled` falls back to one stretched quad at
    // `rect <= inset + 1`, so the fit keeps two cells of centre.
    let margin = 2.0 * MIN_CELL;
    let fit_x = if il + ir > 0.0 {
        (rect.w - margin) / (il + ir)
    } else {
        f32::INFINITY
    };
    let fit_y = if it + ib > 0.0 {
        (rect.h - margin) / (it + ib)
    } else {
        f32::INFINITY
    };
    let effective = s.min(fit_x).min(fit_y).max(MIN_SCALE);
    skin.nine_scaled(batch, role, rect, tint, effective)
}

/// Inset `rect` by `border + pad`, capped at `max_frac` of each side and at the
/// geometric half-minus-a-cell so the result is always strictly inside.
fn inset_content(rect: Rect, border: [f32; 4], pad: f32, max_frac: f32) -> Rect {
    let cap_x = rect.w * max_frac.clamp(0.0, 0.5);
    let cap_y = rect.h * max_frac.clamp(0.0, 0.5);
    let half_w = ((rect.w - MIN_CELL) * 0.5).max(0.0);
    let half_h = ((rect.h - MIN_CELL) * 0.5).max(0.0);
    let left = (border[0] + pad).min(cap_x).min(half_w).max(0.0);
    let right = (border[2] + pad).min(cap_x).min(half_w).max(0.0);
    let top = (border[1] + pad).min(cap_y).min(half_h).max(0.0);
    let bottom = (border[3] + pad).min(cap_y).min(half_h).max(0.0);
    Rect::new(
        rect.x + left,
        rect.y + top,
        (rect.w - left - right).max(0.0),
        (rect.h - top - bottom).max(0.0),
    )
}

/// Content box of a panel: the panel rect inset by its measured border (plus
/// the content gutter). Always strictly inside the panel.
pub fn panel_content(skin: &Skin, rect: Rect, kind: PanelKind, scale: f32) -> Rect {
    let border = border(skin, kind.role(), scale);
    inset_content(rect, border, CONTENT_PAD * scale.max(MIN_SCALE), 0.5)
}

/// Draw a panel of `kind` into `rect` and return its content box.
pub fn draw_panel(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    kind: PanelKind,
    scale: f32,
) -> Rect {
    let s = scale.max(MIN_SCALE);
    nine_scaled(skin, batch, kind.role(), rect, s, kind.tint());
    panel_content(skin, rect, kind, s)
}

/// Shrink `rect` about its centre by `factor` (press feedback).
fn shrink_about_centre(rect: Rect, factor: f32) -> Rect {
    let f = factor.clamp(0.0, 1.0);
    let w = rect.w * f;
    let h = rect.h * f;
    Rect::new(
        rect.x + (rect.w - w) * 0.5,
        rect.y + (rect.h - h) * 0.5,
        w,
        h,
    )
}

/// Draw one button and return the rect its label goes into.
///
/// Pressed uses [`ROLE_BUTTON_PRESSED`] and the 0.96 press shrink with the 8%
/// darkening; disabled keeps the same art and tints it down by 35%.
pub fn draw_button(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    pressed: bool,
    enabled: bool,
    scale: f32,
) -> Rect {
    let s = scale.max(MIN_SCALE);
    let target = if pressed {
        shrink_about_centre(rect, 1.0 - PRESS_SHRINK * 2.0)
    } else {
        rect
    };
    let mut tint = if pressed {
        BUTTON_PRESSED_TINT
    } else {
        BUTTON_TINT
    };
    if !enabled {
        for channel in tint.iter_mut().take(3) {
            *channel *= DISABLED_DIM;
        }
    }
    let role = if pressed {
        ROLE_BUTTON_PRESSED
    } else {
        ROLE_BUTTON
    };
    nine_scaled(skin, batch, role, target, s, tint);
    // The label box follows the button plate, not the pressed plate: the pressed
    // role is a much wider sheet whose bevel would swallow a short button.
    let label_border = border(skin, ROLE_BUTTON, s);
    inset_content(target, label_border, BUTTON_PAD * s, LABEL_INSET_FRAC)
}

/// Horizontal strip of `count` rows inside `content`, drawn on `list_panel`.
///
/// Rows alternate [`ROW_TINT`] / [`ROW_TINT_ALT`]. They always fit: see
/// [`row_pitch`] for the shrink rule. Returns an empty vec when not even a 1 px
/// row fits (the caller then reduces `count` or grows the list).
pub fn draw_rows(
    skin: &Skin,
    batch: &mut UiBatch,
    content: Rect,
    count: usize,
    row_h: f32,
    scale: f32,
) -> Vec<Rect> {
    let s = scale.max(MIN_SCALE);
    if count == 0 || content.w < MIN_CELL || content.h < MIN_ROW {
        return Vec::new();
    }
    let gap = ROW_GAP * s;
    let height = row_pitch(content.h, count, row_h, gap);
    if height < MIN_ROW {
        return Vec::new();
    }
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        let row = Rect::new(
            content.x,
            content.y + index as f32 * (height + gap),
            content.w,
            height,
        );
        let tint = if index % 2 == 0 {
            ROW_TINT
        } else {
            ROW_TINT_ALT
        };
        if !nine_scaled(skin, batch, ROLE_QUEST, row, s, tint) {
            // No atlas: keep the row visible as a dark band so the list still
            // reads while the FDF art is in charge.
            let color = if index % 2 == 0 {
                [0.10, 0.09, 0.08, 0.55]
            } else {
                [0.20, 0.18, 0.15, 0.55]
            };
            batch.rect(row, color);
        }
        rows.push(row);
    }
    rows
}

/// Row height inside `avail` px for `count` rows.
///
/// Rule: `row_h` while `count * row_h + gap * (count - 1)` fits, otherwise the
/// pitch shrinks to the exact fit `(avail - gaps) / count`, so the last row ends
/// on the content bottom edge instead of overflowing it.
pub fn row_pitch(avail: f32, count: usize, row_h: f32, gap: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let gaps = gap.max(0.0) * count.saturating_sub(1) as f32;
    let fit = ((avail - gaps) / count as f32).max(0.0);
    row_h.min(fit)
}

/// Column layout for `count` buttons inside `content`.
///
/// Rule: `columns` per row, `height` tall when the rows fit, shrunk to the row
/// slot when they do not; a short last row is centred. Never overlaps and never
/// leaves `content`. Returns an empty vec when the content cannot host the grid
/// at 1 px per cell (the caller then drops a row or grows the panel).
pub fn layout_buttons(
    content: Rect,
    count: usize,
    columns: usize,
    height: f32,
    gap: f32,
) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let cols = columns.max(1).min(count);
    let rows = (count + cols - 1) / cols;
    let g = gap.max(0.0);
    if content.w < cols as f32 || content.h < rows as f32 {
        return Vec::new();
    }
    let slot_h = (content.h - g * rows.saturating_sub(1) as f32) / rows as f32;
    if slot_h < 1.0 {
        return Vec::new();
    }
    let width = (content.w - g * cols.saturating_sub(1) as f32) / cols as f32;
    if width < 1.0 {
        return Vec::new();
    }
    let h = height.max(1.0).min(slot_h);
    let mut out = Vec::with_capacity(count);
    for row in 0..rows {
        let in_row = (count - row * cols).min(cols);
        let row_w = in_row as f32 * width + in_row.saturating_sub(1) as f32 * g;
        let x0 = content.x + (content.w - row_w) * 0.5;
        let y = content.y + row as f32 * (slot_h + g) + (slot_h - h) * 0.5;
        for col in 0..in_row {
            out.push(Rect::new(x0 + col as f32 * (width + g), y, width, h));
        }
    }
    out
}

/// Draw a title plate and return the rect its text goes into.
pub fn draw_banner(skin: &Skin, batch: &mut UiBatch, rect: Rect, scale: f32) -> Rect {
    let s = scale.max(MIN_SCALE);
    nine_scaled(skin, batch, ROLE_BANNER, rect, s, BANNER_TINT);
    let border = border(skin, ROLE_BANNER, s);
    inset_content(rect, border, BUTTON_PAD * s, LABEL_INSET_FRAC)
}

/// Scale at which `role` fills `w_frac` of the viewport width or `h_frac` of its
/// height — whichever is smaller, so the art keeps its authored proportions.
fn fit_scale(skin: &Skin, role: &str, viewport: Rect, w_frac: f32, h_frac: f32) -> f32 {
    let (aw, ah) = role_size(skin, role).unwrap_or((1.0, 1.0));
    if aw <= 0.0 || ah <= 0.0 || viewport.w <= 0.0 || viewport.h <= 0.0 {
        return 1.0;
    }
    (viewport.w * w_frac / aw)
        .min(viewport.h * h_frac / ah)
        .clamp(MIN_SCALE, MAX_PANEL_SCALE)
}

fn centred(size: Rect, viewport: Rect) -> Rect {
    Rect::new(
        viewport.x + (viewport.w - size.w) * 0.5,
        viewport.y + (viewport.h - size.h) * 0.5,
        size.w,
        size.h,
    )
}

/// Main menu / skirmish panel scale for `viewport` (safe frame, not raw screen).
pub fn menu_scale(skin: &Skin, viewport: Rect) -> f32 {
    let landscape = viewport.w >= viewport.h;
    let (w_frac, h_frac) = if landscape { (0.50, 0.80) } else { (0.90, 0.42) };
    fit_scale(skin, ROLE_MENU, viewport, w_frac, h_frac)
}

/// Game menu / victory panel scale for `viewport`.
pub fn dialog_scale(skin: &Skin, viewport: Rect) -> f32 {
    fit_scale(skin, ROLE_DIALOG, viewport, 0.86, 0.78)
}

/// Quest-log panel scale for `viewport`.
pub fn quest_scale(skin: &Skin, viewport: Rect) -> f32 {
    let landscape = viewport.w >= viewport.h;
    fit_scale(
        skin,
        ROLE_QUEST,
        viewport,
        if landscape { 0.24 } else { 0.86 },
        0.80,
    )
}

/// Panel rect of `role` at `scale`, centred in `viewport`.
pub fn centred_panel(skin: &Skin, role: &str, viewport: Rect, scale: f32) -> Option<Rect> {
    let (aw, ah) = role_size(skin, role)?;
    let s = scale.max(MIN_SCALE);
    Some(centred(Rect::new(0.0, 0.0, aw * s, ah * s), viewport))
}

/// Quest list rect: right-anchored in landscape, centred in portrait.
pub fn quest_rect(skin: &Skin, viewport: Rect, scale: f32) -> Option<Rect> {
    let (aw, ah) = role_size(skin, ROLE_QUEST)?;
    let s = scale.max(MIN_SCALE);
    let size = Rect::new(0.0, 0.0, aw * s, ah * s);
    if viewport.w >= viewport.h {
        let margin = viewport.w * 0.025;
        let x = (viewport.right() - size.w - margin).max(viewport.x);
        Some(Rect::new(x, viewport.y + (viewport.h - size.h) * 0.5, size.w, size.h))
    } else {
        Some(centred(size, viewport))
    }
}

/// Result of [`draw_menu`]: the plate, its content box and the button lanes.
#[derive(Clone, Debug, Default)]
pub struct MenuLayout {
    pub panel: Rect,
    pub content: Rect,
    pub buttons: Vec<Rect>,
    pub labels: Vec<Rect>,
}

/// Result of [`draw_dialog`].
#[derive(Clone, Debug, Default)]
pub struct DialogLayout {
    pub panel: Rect,
    pub content: Rect,
    pub banner: Rect,
    pub banner_text: Rect,
    pub buttons: Vec<Rect>,
    pub labels: Vec<Rect>,
}

/// Result of [`draw_quest_log`].
#[derive(Clone, Debug, Default)]
pub struct QuestLogLayout {
    pub panel: Rect,
    pub content: Rect,
    pub rows: Vec<Rect>,
}

/// Draw the main menu / skirmish setup: plate, then one full-width button per
/// entry in the plate's lanes. `pressed` marks the held entry.
///
/// `scale` is the panel scale from [`menu_scale`]; the plate is drawn at
/// `atlas * scale` so the baked row guides stay undistorted.
pub fn draw_menu(
    skin: &Skin,
    batch: &mut UiBatch,
    viewport: Rect,
    scale: f32,
    entries: usize,
    pressed: Option<usize>,
) -> Option<MenuLayout> {
    let s = scale.max(MIN_SCALE);
    let panel = centred_panel(skin, ROLE_MENU, viewport, s)?;
    let content = draw_panel(skin, batch, panel, PanelKind::Menu, s);
    let gap = MENU_ROW_GAP * s;
    let lane = ((content.h - gap * entries.saturating_sub(1) as f32) / entries.max(1) as f32).max(0.0);
    let buttons = layout_buttons(content, entries, 1, lane, gap);
    let mut labels = Vec::with_capacity(buttons.len());
    for (index, button) in buttons.iter().enumerate() {
        labels.push(draw_button(
            skin,
            batch,
            *button,
            pressed == Some(index),
            true,
            s,
        ));
    }
    Some(MenuLayout {
        panel,
        content,
        buttons,
        labels,
    })
}

/// Draw a game-menu / victory / defeat dialog: plate, title cartouche and a
/// button row at the bottom.
pub fn draw_dialog(
    skin: &Skin,
    batch: &mut UiBatch,
    viewport: Rect,
    scale: f32,
    buttons: usize,
    pressed: Option<usize>,
) -> Option<DialogLayout> {
    let s = scale.max(MIN_SCALE);
    let panel = centred_panel(skin, ROLE_DIALOG, viewport, s)?;
    let content = draw_panel(skin, batch, panel, PanelKind::Dialog, s);

    let aspect = role_size(skin, ROLE_BANNER)
        .map(|(w, h)| if h > 0.0 { w / h } else { 1.0 })
        .unwrap_or(1.0);
    let max_w = (content.w * 0.8).max(MIN_CELL);
    let max_h = (content.h * BANNER_MAX_FRAC).max(MIN_CELL);
    let banner_w = max_w.min(max_h * aspect).max(MIN_CELL);
    let banner = Rect::new(
        content.x + (content.w - banner_w) * 0.5,
        content.y,
        banner_w,
        (banner_w / aspect).max(MIN_CELL),
    );
    let banner_text = draw_banner(skin, batch, banner, s);

    let columns = buttons.min(2).max(1);
    let button_h = (BUTTON_H * s).min(content.h * 0.14).max(MIN_CELL);
    let footer = Rect::new(content.x, content.bottom() - button_h, content.w, button_h);
    let rects = layout_buttons(footer, buttons, columns, button_h, BUTTON_GAP * s);
    let mut labels = Vec::with_capacity(rects.len());
    for (index, button) in rects.iter().enumerate() {
        labels.push(draw_button(
            skin,
            batch,
            *button,
            pressed == Some(index),
            true,
            s,
        ));
    }
    Some(DialogLayout {
        panel,
        content,
        banner,
        banner_text,
        buttons: rects,
        labels,
    })
}

/// Draw the quest log: framed list, then the quest rows inside it.
pub fn draw_quest_log(
    skin: &Skin,
    batch: &mut UiBatch,
    viewport: Rect,
    scale: f32,
    quests: usize,
) -> Option<QuestLogLayout> {
    let s = scale.max(MIN_SCALE);
    let panel = quest_rect(skin, viewport, s)?;
    let content = draw_panel(skin, batch, panel, PanelKind::Quest, s);
    let rows = draw_rows(skin, batch, content, quests, ROW_H * s, s);
    Some(QuestLogLayout {
        panel,
        content,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    /// The synthetic fixture: 12 px insets on every role.
    fn skin() -> Skin {
        sample_skin()
    }

    /// The measured sidecar (`analysis/h2/atlas/ui_atlas_insets.json`) applied to
    /// the same atlas rects. These are the insets the shipped art really has:
    /// `dialog_wide` keeps 113 px of gem header at the top, `list_panel` keeps a
    /// 92 px spine at the left, `button_normal` is a two-plate sheet.
    fn measured_skin() -> Skin {
        let mut skin = sample_skin();
        skin.insets = [
            [17.0, 77.0, 50.0, 82.0],
            [79.0, 77.0, 16.0, 82.0],
            [55.0, 36.0, 55.0, 55.0],
            [66.0, 68.0, 67.0, 56.0],
            [27.0, 23.0, 27.0, 27.0],
            [18.0, 33.0, 33.0, 33.0],
            [49.0, 65.0, 50.0, 48.0],
            [68.0, 40.0, 51.0, 96.0],
            [16.0, 16.0, 16.0, 16.0],
            [41.0, 36.0, 77.0, 57.0],
            [17.0, 38.0, 20.0, 38.0],
            [38.0, 29.0, 29.0, 38.0],
            [38.0, 19.0, 23.0, 38.0],
            [35.0, 38.0, 38.0, 38.0],
            [34.0, 35.0, 34.0, 31.0],
            [18.0, 20.0, 16.0, 17.0],
            [42.0, 26.0, 42.0, 28.0],
            [21.0, 27.0, 21.0, 29.0],
            [28.0, 28.0, 17.0, 28.0],
            [92.0, 68.0, 27.0, 29.0],
            [27.0, 45.0, 24.0, 50.0],
            [45.0, 113.0, 39.0, 56.0],
            [64.0, 88.0, 88.0, 66.0],
            [19.0, 46.0, 21.0, 50.0],
        ];
        skin
    }

    /// One emitted quad: destination rect, UV box, colour.
    #[derive(Clone, Copy, Debug)]
    struct Quad {
        rect: Rect,
        uv: [f32; 4],
        color: [f32; 4],
    }

    fn quads(batch: &UiBatch) -> Vec<Quad> {
        batch
            .textured
            .iter()
            .flat_map(|draw| draw.vertices.chunks(6))
            .map(|v| Quad {
                rect: Rect::new(
                    v[0].pos[0],
                    v[0].pos[1],
                    v[2].pos[0] - v[0].pos[0],
                    v[2].pos[1] - v[0].pos[1],
                ),
                uv: [v[0].uv[0], v[0].uv[1], v[2].uv[0], v[2].uv[1]],
                color: v[0].color,
            })
            .collect()
    }

    fn role_uv(skin: &Skin, role: &str) -> [f32; 4] {
        let index = Skin::role_index(role).expect("role");
        let [ax, ay, aw, ah] = skin.rects[index];
        [
            ax / skin.width,
            ay / skin.height,
            (ax + aw) / skin.width,
            (ay + ah) / skin.height,
        ]
    }

    fn uv_in(uv: &[f32; 4], bound: &[f32; 4]) -> bool {
        uv[0] >= bound[0] - 1e-5
            && uv[1] >= bound[1] - 1e-5
            && uv[2] <= bound[2] + 1e-5
            && uv[3] <= bound[3] + 1e-5
    }

    /// Real margin on every side: the panel content contract.
    ///
    /// Strict `>` / `<` with no epsilon: adding `1e-4` to a screen coordinate
    /// above ~1024 is below one f32 ulp and would silently compare `x < x`.
    fn strictly_inside(inner: Rect, outer: Rect) -> bool {
        inner.x > outer.x
            && inner.y > outer.y
            && inner.right() < outer.right()
            && inner.bottom() < outer.bottom()
            && inner.w > 0.0
            && inner.h > 0.0
    }

    /// Contained, edges may touch: rows and buttons legitimately fill the lane.
    fn inside(inner: Rect, outer: Rect) -> bool {
        let tol = |a: f32, b: f32| 1e-5 * a.abs().max(b.abs()).max(1.0);
        inner.x >= outer.x - tol(inner.x, outer.x)
            && inner.y >= outer.y - tol(inner.y, outer.y)
            && inner.right() <= outer.right() + tol(inner.right(), outer.right())
            && inner.bottom() <= outer.bottom() + tol(inner.bottom(), outer.bottom())
            && inner.w > 0.0
            && inner.h > 0.0
    }

    /// Vertex colours are linear (`hud::linear_color`), so brightness is
    /// measured as emitted light, not as a sum of the encoded channels.
    fn luminance(color: [f32; 4]) -> f32 {
        crate::hud::linear_luminance(color)
    }

    /// Compare a vertex colour against the sRGB-authored tint it came from.
    fn authored(color: [f32; 4]) -> [f32; 4] {
        crate::hud::authored_color(color)
    }

    fn pairwise_overlap(rects: &[Rect]) -> Option<(usize, usize)> {
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                if rects[i].intersects(&rects[j]) {
                    return Some((i, j));
                }
            }
        }
        None
    }

    const LANDSCAPE: Rect = Rect {
        x: 48.0,
        y: 88.0,
        w: 2414.0 - 96.0,
        h: 1080.0 - 88.0 - 52.8,
    };
    const PORTRAIT: Rect = Rect {
        x: 24.0,
        y: 105.6,
        w: 1080.0 - 48.0,
        h: 2414.0 - 105.6 - 70.4,
    };

    #[test]
    fn panel_content_is_strictly_inside_for_every_kind_and_skin() {
        for skin in [skin(), measured_skin()] {
            for kind in PanelKind::ALL {
                for scale in [0.62_f32, 1.0, 1.1, 2.0] {
                    for viewport in [LANDSCAPE, PORTRAIT] {
                        let rect = Rect::new(
                            viewport.x + viewport.w * 0.1,
                            viewport.y + viewport.h * 0.2,
                            viewport.w * 0.42,
                            viewport.h * 0.55,
                        );
                        let mut batch = UiBatch::default();
                        let content = draw_panel(&skin, &mut batch, rect, kind, scale);
                        assert!(
                            strictly_inside(content, rect),
                            "{:?} at {scale}: content {content:?} not inside {rect:?}",
                            kind
                        );
                        assert!(!batch.textured.is_empty(), "{kind:?}: no quads");
                        for quad in quads(&batch) {
                            assert!(
                                quad.rect.x >= rect.x - 0.5
                                    && quad.rect.y >= rect.y - 0.5
                                    && quad.rect.right() <= rect.right() + 0.5
                                    && quad.rect.bottom() <= rect.bottom() + 0.5,
                                "{kind:?}: quad {:?} escapes {rect:?}",
                                quad.rect
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn panel_content_follows_the_measured_border() {
        let skin = measured_skin();
        let rect = Rect::new(10.0, 20.0, 683.0, 648.0);
        for kind in PanelKind::ALL {
            let scale = 2.0_f32;
            let b = border(&skin, kind.role(), scale);
            let pad = CONTENT_PAD * scale;
            let content = panel_content(&skin, rect, kind, scale);
            assert!((content.x - (rect.x + b[0] + pad)).abs() < 1e-3, "{kind:?} left");
            assert!((content.y - (rect.y + b[1] + pad)).abs() < 1e-3, "{kind:?} top");
            assert!(
                (content.right() - (rect.right() - b[2] - pad)).abs() < 1e-3,
                "{kind:?} right"
            );
            assert!(
                (content.bottom() - (rect.bottom() - b[3] - pad)).abs() < 1e-3,
                "{kind:?} bottom"
            );
            assert!(strictly_inside(content, rect), "{kind:?}");
        }
    }

    #[test]
    fn panel_content_survives_a_tiny_panel() {
        let skin = measured_skin();
        for kind in PanelKind::ALL {
            for (w, h) in [(3.0_f32, 3.0_f32), (20.0, 12.0), (200.0, 40.0)] {
                let rect = Rect::new(5.0, 7.0, w, h);
                let content = panel_content(&skin, rect, kind, 1.0);
                assert!(
                    content.x >= rect.x && content.y >= rect.y,
                    "{kind:?} {w}x{h}: origin escaped"
                );
                assert!(
                    content.right() <= rect.right() + 1e-4
                        && content.bottom() <= rect.bottom() + 1e-4,
                    "{kind:?} {w}x{h}: extent escaped"
                );
                assert!(content.w >= 0.0 && content.h >= 0.0, "{kind:?}: negative");
            }
        }
    }

    #[test]
    fn nine_patch_corner_identity_at_1x_and_2x() {
        let skin = skin();
        let rect = Rect::new(100.0, 60.0, 400.0, 300.0);
        let one = {
            let mut batch = UiBatch::default();
            assert!(nine_scaled(&skin, &mut batch, ROLE_DIALOG, rect, 1.0, [1.0; 4]));
            quads(&batch)
        };
        let two = {
            let mut batch = UiBatch::default();
            assert!(nine_scaled(&skin, &mut batch, ROLE_DIALOG, rect, 2.0, [1.0; 4]));
            quads(&batch)
        };
        assert_eq!(one.len(), 9, "1x must be nine cells");
        assert_eq!(two.len(), 9, "2x must be nine cells");
        let inset = skin.insets[Skin::role_index(ROLE_DIALOG).expect("role")];
        // Top-left corner: same texels, DPI-scaled destination size.
        assert_eq!(one[0].uv, two[0].uv, "corner UV must be scale-free");
        assert!((one[0].rect.w - inset[0]).abs() < 1e-4, "1x corner width");
        assert!((two[0].rect.w - inset[0] * 2.0).abs() < 1e-4, "2x corner width");
        assert!((one[0].rect.h - inset[1]).abs() < 1e-4, "1x corner height");
        assert!((two[0].rect.h - inset[1] * 2.0).abs() < 1e-4, "2x corner height");
        // The corner is a corner, not the whole plate.
        assert_ne!(one[0].uv, one[1].uv, "corner must not stretch the edge");
        // Cells partition the destination at both scales.
        for cells in [&one, &two] {
            let top = cells[0].rect.w + cells[1].rect.w + cells[2].rect.w;
            let left = cells[0].rect.h + cells[3].rect.h + cells[6].rect.h;
            assert!((top - rect.w).abs() < 1e-3, "row partition {top} vs {}", rect.w);
            assert!((left - rect.h).abs() < 1e-3, "column partition {left} vs {}", rect.h);
        }
        // At 2x the centre gives ground to the border; the art itself grew.
        assert!(two[4].rect.w > 0.0 && two[4].rect.h > 0.0, "2x centre");
        assert!(one[4].rect.w > two[4].rect.w, "2x border takes more of the rect");
    }

    #[test]
    fn nine_scaled_matches_skin_nine_at_1x() {
        let skin = measured_skin();
        let rect = Rect::new(20.0, 30.0, 900.0, 700.0);
        for role in [ROLE_MENU, ROLE_DIALOG, ROLE_QUEST, ROLE_BUTTON, ROLE_BANNER] {
            let mut mine = UiBatch::default();
            assert!(nine_scaled(&skin, &mut mine, role, rect, 1.0, [0.9, 0.8, 0.7, 1.0]));
            let mut reference = UiBatch::default();
            assert!(skin.nine(&mut reference, role, rect, [0.9, 0.8, 0.7, 1.0]));
            let a = quads(&mine);
            let b = quads(&reference);
            assert_eq!(a.len(), b.len(), "{role}: quad count");
            for (lhs, rhs) in a.iter().zip(b.iter()) {
                assert!((lhs.rect.x - rhs.rect.x).abs() < 1e-4, "{role}: x");
                assert!((lhs.rect.y - rhs.rect.y).abs() < 1e-4, "{role}: y");
                assert!((lhs.rect.w - rhs.rect.w).abs() < 1e-4, "{role}: w");
                assert!((lhs.rect.h - rhs.rect.h).abs() < 1e-4, "{role}: h");
                assert_eq!(lhs.uv, rhs.uv, "{role}: uv");
                assert_eq!(lhs.color, rhs.color, "{role}: tint");
            }
        }
    }

    #[test]
    fn nine_scaled_clamps_instead_of_squeezing_the_sheet() {
        let skin = measured_skin();
        // A quest row shorter than the list bevel: still nine cells, centre kept.
        let rect = Rect::new(0.0, 0.0, 300.0, 40.0);
        let mut batch = UiBatch::default();
        assert!(nine_scaled(&skin, &mut batch, ROLE_QUEST, rect, 1.0, [1.0; 4]));
        let cells = quads(&batch);
        assert_eq!(cells.len(), 9, "clamped nine-patch keeps every cell");
        let centre = cells[4].rect;
        assert!(centre.w >= MIN_CELL - 1e-4 && centre.h >= MIN_CELL - 1e-4);
        assert!(centre.w > 1.0, "row centre must stay usable, got {centre:?}");
        // And nothing leaves the row.
        for cell in cells {
            assert!(cell.rect.x >= -0.01 && cell.rect.right() <= rect.w + 0.01);
            assert!(cell.rect.y >= -0.01 && cell.rect.bottom() <= rect.h + 0.01);
        }
    }

    #[test]
    fn panels_use_their_own_role() {
        let skin = skin();
        for kind in PanelKind::ALL {
            let mut batch = UiBatch::default();
            let rect = Rect::new(0.0, 0.0, 500.0, 400.0);
            draw_panel(&skin, &mut batch, rect, kind, 1.0);
            let bound = role_uv(&skin, kind.role());
            for quad in quads(&batch) {
                assert!(
                    uv_in(&quad.uv, &bound),
                    "{:?}: UV {:?} outside {:?} {:?}",
                    kind,
                    quad.uv,
                    kind.role(),
                    bound
                );
            }
        }
        let menu = role_uv(&skin, PanelKind::Menu.role());
        let dialog = role_uv(&skin, PanelKind::Dialog.role());
        let quest = role_uv(&skin, PanelKind::Quest.role());
        assert_ne!(menu, dialog);
        assert_ne!(menu, quest);
        assert_ne!(dialog, quest);
    }

    #[test]
    fn pressed_and_normal_use_different_roles() {
        let skin = skin();
        let rect = Rect::new(200.0, 100.0, 320.0, 72.0);
        let mut normal = UiBatch::default();
        let normal_label = draw_button(&skin, &mut normal, rect, false, true, 1.0);
        let mut pressed = UiBatch::default();
        let pressed_label = draw_button(&skin, &mut pressed, rect, true, true, 1.0);

        let normal_quads = quads(&normal);
        let pressed_quads = quads(&pressed);
        for quad in &normal_quads {
            assert!(uv_in(&quad.uv, &role_uv(&skin, ROLE_BUTTON)), "normal role");
        }
        for quad in &pressed_quads {
            assert!(
                uv_in(&quad.uv, &role_uv(&skin, ROLE_BUTTON_PRESSED)),
                "pressed role"
            );
        }
        assert_ne!(
            normal_quads[0].uv, pressed_quads[0].uv,
            "states must not share art"
        );
        // Press shrink: 0.96 of the rect about its centre.
        let first = pressed_quads
            .iter()
            .map(|q| q.rect)
            .reduce(|a, b| Rect::new(a.x.min(b.x), a.y.min(b.y), 0.0, 0.0))
            .expect("pressed quads");
        assert!((first.x - (rect.x + rect.w * PRESS_SHRINK)).abs() < 1e-3);
        assert!(
            (pressed_label.w - normal_label.w).abs() > 0.0,
            "label follows the shrunk plate"
        );
        assert!(strictly_inside(normal_label, rect), "normal label inside");
        assert!(strictly_inside(pressed_label, rect), "pressed label inside");
    }

    #[test]
    fn disabled_tint_is_strictly_darker() {
        let skin = skin();
        let rect = Rect::new(0.0, 0.0, 260.0, 80.0);
        let mut enabled = UiBatch::default();
        draw_button(&skin, &mut enabled, rect, false, true, 1.0);
        let mut disabled = UiBatch::default();
        draw_button(&skin, &mut disabled, rect, false, false, 1.0);
        let on = quads(&enabled)[4].color;
        let off = quads(&disabled)[4].color;
        assert!(luminance(off) < luminance(on), "disabled must be darker");
        // `DISABLED_DIM` is authored in sRGB (it multiplies the tint before the
        // vertex colour becomes linear), so it is exact on the authored value,
        // not on the linear luminance: a 0.65 sRGB dim is a ~0.36 light ratio.
        let on_authored = authored(on);
        let off_authored = authored(off);
        for channel in 0..3 {
            let ratio = off_authored[channel] / on_authored[channel];
            assert!(
                (ratio - DISABLED_DIM).abs() < 1e-5,
                "authored channel {channel} ratio {ratio} != {DISABLED_DIM}"
            );
        }
        assert!(
            luminance(off) / luminance(on) < DISABLED_DIM,
            "an sRGB dim removes more light than a linear one"
        );
        assert!((off[3] - 1.0).abs() < 1e-6, "disabled keeps the alpha");
        // Pressed + disabled is strictly darker than pressed.
        let mut pressed = UiBatch::default();
        draw_button(&skin, &mut pressed, rect, true, true, 1.0);
        let mut both = UiBatch::default();
        draw_button(&skin, &mut both, rect, true, false, 1.0);
        assert!(luminance(quads(&both)[4].color) < luminance(quads(&pressed)[4].color));
    }

    /// `ui.colour.dialogs-disabled-tint` — disabling a button dims the authored
    /// tint by [`DISABLED_DIM`] in sRGB space, which lowers linear luminance for
    /// every tint (that is what "strictly darker" means after the surface writes
    /// linear colours). Pressed stays darker than enabled at every tint too.
    #[test]
    fn ui_colour_dialogs_disabled_tint_keeps_the_authored_ratio() {
        let skin = skin();
        let rect = Rect::new(12.0, 24.0, 260.0, 80.0);
        let tints = [
            [1.0, 0.98, 0.93, 1.0],
            [0.9, 0.8, 0.7, 1.0],
            [0.55, 0.62, 0.71, 1.0],
        ];
        for pressed in [false, true] {
            for tint in tints {
                let mut on_batch = UiBatch::default();
                let mut off_batch = UiBatch::default();
                draw_button(&skin, &mut on_batch, rect, pressed, true, 1.0);
                draw_button(&skin, &mut off_batch, rect, pressed, false, 1.0);
                let on = authored(quads(&on_batch)[4].color);
                let off = authored(quads(&off_batch)[4].color);
                for channel in 0..3 {
                    assert!(
                        (off[channel] - on[channel] * DISABLED_DIM).abs() < 1e-5,
                        "pressed={pressed} tint={tint:?} channel {channel}: {} != {} * {DISABLED_DIM}",
                        off[channel],
                        on[channel]
                    );
                }
                assert_eq!(off[3], on[3], "alpha is not dimmed");
                let linear_on = quads(&on_batch)[4].color;
                let linear_off = quads(&off_batch)[4].color;
                assert!(
                    luminance(linear_off) < luminance(linear_on),
                    "pressed={pressed} tint={tint:?}: disabled must emit less light"
                );
            }
        }
        // Pressed is a different plate: it cannot be lighter than the idle one.
        let mut idle = UiBatch::default();
        let mut pressed = UiBatch::default();
        draw_button(&skin, &mut idle, rect, false, true, 1.0);
        draw_button(&skin, &mut pressed, rect, true, true, 1.0);
        assert!(luminance(quads(&pressed)[4].color) < luminance(quads(&idle)[4].color));
    }

    #[test]
    fn buttons_never_overlap_for_3_6_8_rows() {
        let content = Rect::new(40.0, 60.0, 900.0, 520.0);
        for count in [3_usize, 6, 8] {
            for columns in [1_usize, 2, 3] {
                let rects = layout_buttons(content, count, columns, 64.0, 8.0);
                assert_eq!(rects.len(), count, "{count}/{columns}: must place every button");
                assert!(
                    pairwise_overlap(&rects).is_none(),
                    "{count}/{columns}: overlap {:?}",
                    pairwise_overlap(&rects)
                );
                for rect in &rects {
                    assert!(
                        rect.x >= content.x - 1e-3
                            && rect.y >= content.y - 1e-3
                            && rect.right() <= content.right() + 1e-3
                            && rect.bottom() <= content.bottom() + 1e-3,
                        "{count}/{columns}: button {rect:?} outside {content:?}"
                    );
                    assert!(rect.w >= 1.0 && rect.h >= 1.0);
                }
            }
        }
        // Shrink rule: 8 rows of 64 do not fit 520 px, so the height is the slot.
        let rects = layout_buttons(content, 8, 1, 64.0, 8.0);
        let slot = (520.0 - 7.0 * 8.0) / 8.0;
        assert!((rects[0].h - slot).abs() < 1e-3, "8 rows shrink to the slot");
        assert!((rects[0].h - 64.0).abs() > 1.0, "and they really did shrink");
        // 3 rows keep the authored height.
        let rects = layout_buttons(content, 3, 1, 64.0, 8.0);
        assert!((rects[0].h - 64.0).abs() < 1e-3, "3 rows keep 64 px");
    }

    #[test]
    fn rows_fit_or_shrink_by_the_documented_rule() {
        let skin = skin();
        let content = Rect::new(100.0, 80.0, 640.0, 500.0);
        for count in [1_usize, 2, 4, 6, 8, 12] {
            for row_h in [40.0_f32, 64.0, 220.0] {
                let mut batch = UiBatch::default();
                let rows = draw_rows(&skin, &mut batch, content, count, row_h, 1.0);
                let gap = ROW_GAP;
                let expected = row_pitch(content.h, count, row_h, gap);
                assert_eq!(rows.len(), count, "{count} rows of {row_h}");
                assert!((rows[0].h - expected).abs() < 1e-3, "pitch rule");
                assert!(
                    pairwise_overlap(&rows).is_none(),
                    "{count} rows of {row_h} overlap"
                );
                for (index, row) in rows.iter().enumerate() {
                    assert!(
                        row.y >= content.y - 1e-3
                            && row.bottom() <= content.bottom() + 1e-3
                            && row.right() <= content.right() + 1e-3,
                        "{count}/{row_h} row {index} {row:?} outside {content:?}"
                    );
                }
                let last = rows.last().expect("row");
                if rows[0].h < row_h - 1e-6 {
                    // The documented shrink rule: a shrunk list ends flush.
                    assert!(
                        (last.bottom() - content.bottom()).abs() < 1e-3,
                        "{count}/{row_h}: a shrunk list must end on the content bottom"
                    );
                } else {
                    // A list that fits keeps its authored height and stops early.
                    assert!(
                        last.bottom() <= content.bottom() + 1e-3,
                        "{count}/{row_h}: a fitting list must not overflow"
                    );
                }
            }
        }
        // Oversubscribed: 12 rows of 220 cannot fit, so the pitch is the exact fit.
        let mut batch = UiBatch::default();
        let rows = draw_rows(&skin, &mut batch, content, 12, 220.0, 1.0);
        let fit = (content.h - ROW_GAP * 11.0) / 12.0;
        assert!((rows[0].h - fit).abs() < 1e-3, "12 rows shrink to the exact fit");
        assert!((rows[11].bottom() - content.bottom()).abs() < 1e-3, "last row flush");
        // Rows that cannot be placed are reported, not overlapped.
        let tiny = Rect::new(10.0, 10.0, 50.0, 0.5);
        let empty = draw_rows(&skin, &mut batch, tiny, 4, 10.0, 1.0);
        assert!(empty.is_empty(), "no row fits 0.5 px, must report empty");
    }

    #[test]
    fn rows_alternate_tint() {
        let skin = skin();
        let content = Rect::new(0.0, 0.0, 400.0, 400.0);
        let mut batch = UiBatch::default();
        let rows = draw_rows(&skin, &mut batch, content, 4, 90.0, 1.0);
        assert_eq!(rows.len(), 4);
        let colors = quads(&batch);
        // Each row is its own nine-patch (9 quads): compare the row centres.
        assert_eq!(colors.len(), 36, "four nine-patch rows");
        let first = colors[4].color;
        let second = colors[13].color;
        assert_ne!(first, second, "alternating tint");
        assert!(luminance(second) < luminance(first), "odd rows are darker");
        assert_eq!(colors[22].color, first, "row 2 returns to the base tint");
    }

    #[test]
    fn menu_composition_stays_inside_the_viewport() {
        for skin in [skin(), measured_skin()] {
            for viewport in [LANDSCAPE, PORTRAIT] {
                let scale = menu_scale(&skin, viewport);
                assert!(scale > 0.0);
                let mut batch = UiBatch::default();
                let menu = draw_menu(&skin, &mut batch, viewport, scale, 4, Some(1))
                    .expect("menu layout");
                assert!(
                    strictly_inside(menu.panel, viewport),
                    "panel {:?} inside {:?}",
                    menu.panel,
                    viewport
                );
                assert!(strictly_inside(menu.content, menu.panel), "content inside panel");
                assert_eq!(menu.buttons.len(), 4);
                assert_eq!(menu.labels.len(), 4);
                assert!(pairwise_overlap(&menu.buttons).is_none(), "menu rows overlap");
                for (button, label) in menu.buttons.iter().zip(menu.labels.iter()) {
                    assert!(inside(*button, menu.content), "button in content");
                    assert!(strictly_inside(*label, *button), "label in button");
                }
                // 6 and 8 entries still tile without overlap.
                for entries in [3_usize, 6, 8] {
                    let menu = draw_menu(&skin, &mut batch, viewport, scale, entries, None)
                        .expect("menu layout");
                    assert_eq!(menu.buttons.len(), entries);
                    assert!(
                        pairwise_overlap(&menu.buttons).is_none(),
                        "{entries} entries overlap"
                    );
                    for button in &menu.buttons {
                        assert!(button.bottom() <= menu.content.bottom() + 1e-3);
                        assert!(button.h >= 1.0);
                    }
                }
            }
        }
    }

    #[test]
    fn dialog_composition_keeps_banner_and_buttons_inside() {
        let skin = measured_skin();
        for viewport in [LANDSCAPE, PORTRAIT] {
            let scale = dialog_scale(&skin, viewport);
            let mut batch = UiBatch::default();
            let dialog = draw_dialog(&skin, &mut batch, viewport, scale, 2, Some(0))
                .expect("dialog layout");
            assert!(strictly_inside(dialog.panel, viewport));
            assert!(strictly_inside(dialog.content, dialog.panel));
            assert!(
                dialog.banner.y >= dialog.content.y - 1e-3
                    && dialog.banner.bottom() <= dialog.content.bottom() + 1e-3,
                "banner inside content"
            );
            assert!(strictly_inside(dialog.banner_text, dialog.banner), "title box");
            assert_eq!(dialog.buttons.len(), 2);
            assert!(pairwise_overlap(&dialog.buttons).is_none());
            for button in &dialog.buttons {
                assert!(button.y >= dialog.content.y - 1e-3);
                assert!(button.bottom() <= dialog.content.bottom() + 1e-3);
                assert!(inside(*button, dialog.content));
            }
            // One-button and three-button dialogs too.
            for count in [1_usize, 3] {
                let dialog = draw_dialog(&skin, &mut batch, viewport, scale, count, None)
                    .expect("dialog layout");
                assert_eq!(dialog.buttons.len(), count);
                assert!(pairwise_overlap(&dialog.buttons).is_none(), "{count} buttons");
            }
        }
    }

    #[test]
    fn quest_log_rows_sit_inside_the_list_frame() {
        let skin = measured_skin();
        for viewport in [LANDSCAPE, PORTRAIT] {
            let scale = quest_scale(&skin, viewport);
            let mut batch = UiBatch::default();
            let log = draw_quest_log(&skin, &mut batch, viewport, scale, 6).expect("quest log");
            assert!(strictly_inside(log.panel, viewport));
            assert!(strictly_inside(log.content, log.panel));
            assert_eq!(log.rows.len(), 6, "six quests");
            assert!(pairwise_overlap(&log.rows).is_none());
            for row in &log.rows {
                assert!(
                inside(*row, log.content),
                "row {row:?} outside the list content {:?}",
                log.content
            );
            }
            // The rows are `list_panel` art, not flat bands.
            for quad in quads(&batch) {
                assert!(
                    uv_in(&quad.uv, &role_uv(&skin, ROLE_QUEST)),
                    "row quad outside the list role"
                );
            }
        }
    }

    /// The numbers quoted in `analysis/hudskin/dialogs.md` and the lane report.
    #[test]
    fn layout_evidence_for_2414x1080_and_1080x2414() {
        let skin = measured_skin();
        for (name, viewport) in [("2414x1080", LANDSCAPE), ("1080x2414", PORTRAIT)] {
            let mut batch = UiBatch::default();
            let menu_scale = menu_scale(&skin, viewport);
            let menu = draw_menu(&skin, &mut batch, viewport, menu_scale, 4, Some(1))
                .expect("menu");
            let dialog_scale = dialog_scale(&skin, viewport);
            let dialog = draw_dialog(&skin, &mut batch, viewport, dialog_scale, 2, Some(0))
                .expect("dialog");
            let quest_scale = quest_scale(&skin, viewport);
            let quest = draw_quest_log(&skin, &mut batch, viewport, quest_scale, 6)
                .expect("quest");

            println!(
                "{name}: menu scale={menu_scale:.3} panel={:?} content={:?} button0={:?} label0={:?}",
                menu.panel, menu.content, menu.buttons[0], menu.labels[0]
            );
            println!(
                "          dialog scale={dialog_scale:.3} panel={:?} content={:?} banner={:?} button0={:?} label0={:?}",
                dialog.panel, dialog.content, dialog.banner, dialog.buttons[0], dialog.labels[0]
            );
            println!(
                "          quest scale={quest_scale:.3} panel={:?} content={:?} row0={:?} row_last={:?}",
                quest.panel,
                quest.content,
                quest.rows[0],
                quest.rows.last().expect("row")
            );

            // The evidence must satisfy the contract it claims.
            assert!(menu_scale > 0.0 && dialog_scale > 0.0 && quest_scale > 0.0);
            assert!(strictly_inside(menu.panel, viewport));
            assert!(strictly_inside(dialog.panel, viewport));
            assert!(strictly_inside(quest.panel, viewport));
            assert_eq!(menu.buttons.len(), 4);
            assert_eq!(dialog.buttons.len(), 2);
            assert_eq!(quest.rows.len(), 6);
        }
    }
}
