//! Army panel: the retail selected-group portrait row above the command card,
//! plus the four top-bar tabs — sized for touch, not for a keyboard.
//!
//! Retail WC3 (reference screenshot `images (4).jpeg`) draws the selection as a
//! row of framed unit portraits above the command card — one portrait per
//! selected unit, each with its own HP bar, casters with a thin mana bar, the
//! active/primary unit clearly highlighted — and the top bar starts with
//! `Quests | Menu | Allies | Log`.
//!
//! This target is a phone: **there is no keyboard**, so every entry point is a
//! touch button. The four top-bar entries are tiles with a ≥48 dp target
//! (126 px at density 2.625), and the army row paginates instead of shrinking
//! below that target. The F-keys survive only as tooltip text for keyboard
//! users ([`tab_key_label`] / [`tab_tooltip`]); they are never the way to open
//! a screen.
//!
//! The module is pure geometry + quads: it never resolves textures or glyphs.
//! The caller paints the unit portrait into [`chip_icon`], the tab labels into
//! the returned tab rects, and the page counter into [`page_label_rect`].
//!
//! Wiring (one line in `hud.rs`, owned by the HUD integration lane):
//!
//! ```ignore
//! pub mod army_panel; // resolves to src/hud/army_panel.rs
//! ```
//!
//! Call site: `mobile_hud::draw_army_chips` (`crates/wc3-game/src/mobile_hud.rs:3799`),
//! `rect = MobileHudLayout::army`, `units = HudView::army` mapped 1:1. The four
//! tabs are drawn where the top strip is drawn (`mobile_hud.rs:1937`); the map
//! name then starts at `tabs[3].right() + pad` (the `map_name_x` argument of
//! `topbar::draw` already takes it).
//!
//! ## Sizing rules (measured on this device, 2414x1080, density 2.625)
//!
//! * 48 dp = 126 px. Every interactive rect this module returns is ≥126 px on
//!   its smaller side: [`ArmyParts::hits`], [`ArmyParts::order_hits`] and the
//!   rects of [`draw_top_tabs`].
//! * A chip's art is `min(max(56 * scale, 126), inner_h)` and its hit rect is
//!   padded to the full 126 px slot, so a 103 px panel still gets finger-sized
//!   targets without stretching the portrait past the panel.
//! * Chips never shrink below the touch slot: the row paginates instead.
//!   `per_page = floor((avail + gap) / (visible + gap))`, minus one slot when a
//!   page button is needed, `pages = ceil(count / per_page)`.

use glam::Vec2;

use crate::game::UiAction;
use crate::hud::{Rect, UiBatch};
use crate::hudskin::{army, Skin};

// ---------------------------------------------------------------------------
// Touch policy
// ---------------------------------------------------------------------------

/// Operator's device: 48 dp at density 2.625 = 126 px.
pub const TOUCH_DP: f32 = 48.0;
/// Measured display density of the target device (2414x1080 panel).
pub const DEVICE_DENSITY: f32 = 2.625;
/// The 48 dp target in physical pixels on the target device.
pub const TOUCH_MIN_PX: f32 = 126.0;

/// Touch target policy: the minimum smaller side of an interactive rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchMetrics {
    pub min_side: f32,
}

impl TouchMetrics {
    /// 48 dp on the operator's device (126 px).
    pub const DEVICE: TouchMetrics = TouchMetrics {
        min_side: TOUCH_MIN_PX,
    };

    /// 48 dp on any display: `dp * density` px, never below one pixel.
    pub fn from_dp(dp: f32, density: f32) -> Self {
        let side = if dp.is_finite() && density.is_finite() {
            (dp * density).max(1.0)
        } else {
            TOUCH_MIN_PX
        };
        Self { min_side: side }
    }

    pub fn min_side(self) -> f32 {
        if self.min_side.is_finite() {
            self.min_side.max(1.0)
        } else {
            TOUCH_MIN_PX
        }
    }
}

impl Default for TouchMetrics {
    fn default() -> Self {
        Self::DEVICE
    }
}

// ---------------------------------------------------------------------------
// Army panel constants
// ---------------------------------------------------------------------------

/// Chip art side (screen px) at HUD scale 1; the touch slot can raise it.
pub const CHIP_UNIT: f32 = 56.0;
/// Gap between chips as a fraction of the chip art (retail: 5 px on 34 px).
pub const GAP_RATIO: f32 = 0.15;
/// Frame padding at scale 1.
pub const PANEL_PAD: f32 = 5.0;
/// The page button's glyph, drawn by the caller inside [`page_label_rect`].
pub const PAGE_GLYPH: &str = ">";

/// Panel plate tint (multiplied by the caller tint). The plate is the console
/// sub-panel, so it must read darker than the art and let the chips dominate.
pub const FRAME_TINT: [f32; 4] = [0.30, 0.28, 0.24, 0.92];
/// Gold hairline closing the plate's bottom edge.
pub const FRAME_LINE: [f32; 4] = [0.72, 0.60, 0.32, 0.85];
/// Hairline thickness at scale 1.
pub const LINE_H: f32 = 1.0;
/// Portrait well: the plate the caller's portrait texture covers.
pub const ICON_WELL: [f32; 4] = [0.05, 0.045, 0.04, 0.98];
/// Portrait well when the unit has no icon key (`ArmyChip::icon == None`).
pub const ICON_WELL_EMPTY: [f32; 4] = [0.11, 0.11, 0.12, 0.98];
/// Mana trough (thin bar above the HP bar).
pub const MANA_TRACK: [f32; 4] = [0.02, 0.03, 0.06, 0.92];
/// Mana fill.
pub const MANA_FILL: [f32; 4] = [0.22, 0.42, 0.92, 1.0];
/// Mana bar height as a fraction of the HP bar (a hairline, not a bar).
pub const MANA_RATIO: f32 = 0.55;
/// Primary-chip marker under the frame.
pub const SELECT_MARK: [f32; 4] = [0.95, 0.80, 0.35, 1.0];
/// Primary-chip marker height at scale 1.
pub const MARK_H: f32 = 1.5;
/// Mild brightness lift on the primary chip frame (the gold role is the
/// primary signal; the tint keeps it from washing out under a faded HUD).
pub const SELECT_TINT: [f32; 4] = [1.06, 1.03, 0.96, 1.0];
/// The 216x118 console sub-panel (FDF name: `small_panel` / `strip_top` band).
pub const ROLE_PANEL: &str = "wide_panel";

// ---------------------------------------------------------------------------
// Top-bar tabs
// ---------------------------------------------------------------------------

/// Tab indices, left to right.
pub const TAB_QUESTS: usize = 0;
pub const TAB_MENU: usize = 1;
pub const TAB_ALLIES: usize = 2;
pub const TAB_JOURNAL: usize = 3;
/// Number of top-bar tabs.
pub const TAB_COUNT: usize = 4;

/// Tab labels for the RU client.
pub const TAB_LABELS_RU: [&str; TAB_COUNT] = ["КВЕСТЫ", "МЕНЮ", "АЛЬЯНС", "ЖУРНАЛ"];
/// Tab labels for the EN client.
pub const TAB_LABELS_EN: [&str; TAB_COUNT] = ["QUESTS", "MENU", "ALLIES", "LOG"];

/// Keyboard shortcut per tab, kept **only** for the desktop tooltip. Empty =
/// no keyboard path exists for that entry.
pub const TAB_KEYS: [&str; TAB_COUNT] = ["F9", "F10", "", "F12"];

/// Client language for the tab labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TabLang {
    #[default]
    Ru,
    En,
}

/// Inactive tab plate: the near-square framed slot (102x99 art).
pub const ROLE_TAB: &str = "slot_dark";
/// Active tab plate (gold frame, same 102x99 art).
pub const ROLE_TAB_ACTIVE: &str = "slot_gold";
/// Gap between tab tiles at scale 1.
pub const TAB_GAP: f32 = 8.0;

/// `strip_top`: the top 216x27 band of the 216x118 panel (same crop
/// `assets.rs` maps to the FDF sub-slice `strip_top`). It is drawn as the band
/// under the tab row, at the authored 27 px height.
pub const STRIP_TOP_PARENT: [f32; 2] = [216.0, 118.0];
pub const STRIP_TOP_DY: f32 = 17.0;
pub const STRIP_TOP_H: f32 = 27.0;
/// `button_normal` part0: the normal plate of the 217x119 button sheet, used
/// for the army page button (its authored aspect is 8.7:1, so it is drawn at
/// that aspect inside the square touch slot instead of being squashed).
pub const BUTTON_PART0_PARENT: [f32; 2] = [217.0, 119.0];
pub const BUTTON_PART0_DY: f32 = 15.0;
pub const BUTTON_PART0_H: f32 = 25.0;
/// `button_normal` part1: the gold/pressed plate (y 55..85).
pub const BUTTON_PART1_DY: f32 = 55.0;
pub const BUTTON_PART1_H: f32 = 30.0;
/// A sub-slice is only applied when the parent really is the authored sheet.
pub const SUBSLICE_TOLERANCE: f32 = 4.0;

/// What a top-bar tab opens — the same path the keyboard entry uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabCommand {
    /// The `UiAction::QuestLog` the F1/F9 handler activates (`game.rs:3700`).
    QuestLog,
    /// The pause-menu branch F10/BACK calls (`Game::open_game_menu` /
    /// `Game::resume_battle`, `game.rs:3703`).
    GameMenu,
    /// Ally panel: no ally/diplomacy screen exists in the sim yet, so the tab
    /// is drawn disabled (`tab_enabled` is false) and the caller skips the tap.
    Allies,
    /// Console log/journal: no log screen exists yet; drawn disabled.
    Journal,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// One selected unit in the army panel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArmyChip<'a> {
    /// Portrait icon key (`assets.icon_name_for_def`), `None` for a unit whose
    /// portrait failed to resolve — the chip then shows the empty well.
    pub icon: Option<&'a str>,
    /// HP ratio `0..=1` for the bar.
    pub hp: f32,
    /// Mana ratio `0..=1`; `Some` only for casters, which get the thin bar.
    pub mana: Option<f32>,
    /// Unit definition id (`UnitDef::id`), carried so a chip slot can be
    /// resolved back to the unit a tap selects without a second slice.
    pub def: &'static str,
}

/// One row of chips: the visible art, the touch rects, and the page state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArmyParts {
    /// The panel plate (the caller's rect).
    pub frame: Rect,
    /// Visible chip art, one per unit on the current page.
    pub chips: Vec<Rect>,
    /// Touch rect for each chip (same length as `chips`, ≥ touch minimum).
    pub hits: Vec<Rect>,
    /// Visible page-button art (empty when everything fits on one page).
    pub order_buttons: Vec<Rect>,
    /// Touch rect for each page button.
    pub order_hits: Vec<Rect>,
    /// Current page, clamped into `0..pages`.
    pub page: usize,
    /// Total pages (`1` for a non-empty selection that fits, `0` when empty).
    pub pages: usize,
    /// Absolute index of `chips[0]` in the caller's unit slice.
    pub first: usize,
}

impl ArmyParts {
    pub fn len(&self) -> usize {
        self.chips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chips.is_empty()
    }

    /// Visible art of the page slot `slot`.
    pub fn chip(&self, slot: usize) -> Option<Rect> {
        self.chips.get(slot).copied()
    }

    /// Touch rect of the page slot `slot`.
    pub fn hit_rect(&self, slot: usize) -> Option<Rect> {
        self.hits.get(slot).copied()
    }

    /// Absolute unit index of the page slot `slot`.
    pub fn unit_index(&self, slot: usize) -> Option<usize> {
        self.chip(slot).map(|_| self.first + slot)
    }

    /// Counter the caller draws on the page button (`"1/3"`), empty on one page.
    pub fn page_label(&self) -> String {
        if self.pages <= 1 {
            String::new()
        } else {
            format!("{}/{}", self.page + 1, self.pages)
        }
    }
}

/// What a point inside the army panel means in sim terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmyHit {
    /// A unit portrait (page slot): left tap focuses that unit, right/long tap
    /// drops it from the group (`analysis/hudskin/army_panel.md` §4).
    Chip(usize),
    /// The page button: advance to the next page (wraps).
    Page(usize),
    None,
}

// ---------------------------------------------------------------------------
// Pure layout
// ---------------------------------------------------------------------------

fn sane_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.max(0.05)
    } else {
        1.0
    }
}

fn mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

/// How many equal advances fit in `avail` (`k * advance - gap <= avail`).
fn fit_slots(avail: f32, advance: f32, gap: f32) -> usize {
    if !(avail > 0.0) || !(advance > 0.0) {
        return 0;
    }
    (((avail + gap) / advance).floor()).max(0.0) as usize
}

/// The row geometry of one page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArmyLayout {
    pub frame: Rect,
    /// Visible chip art for this page.
    pub slots: Vec<Rect>,
    /// Touch rect per visible chip.
    pub hits: Vec<Rect>,
    /// `(art, hit)` of the page button, when the row needs one and can host it.
    pub page_button: Option<(Rect, Rect)>,
    pub page: usize,
    pub pages: usize,
    pub per_page: usize,
    pub first: usize,
    /// Chip art side after the touch floor and the panel height are applied.
    pub visible: f32,
    /// Centre-to-centre distance of two chips (never below the touch minimum).
    pub advance: f32,
}

/// Lay out `count` chips in `rect` for `page`.
///
/// A chip is `visible = min(max(56 * scale, touch), inner_h)` square; the row
/// advance is `visible + max(visible * 0.15, touch - visible)`, so two touch
/// rects can never overlap. When the chips do not fit on one page the last slot
/// is given to the page button and `pages = ceil(count / per_page)`.
pub fn layout(
    rect: Rect,
    count: usize,
    page: usize,
    scale: f32,
    touch: TouchMetrics,
) -> ArmyLayout {
    let s = sane_scale(scale);
    let touch_side = touch.min_side();
    let mut out = ArmyLayout {
        frame: rect,
        pages: if count == 0 { 0 } else { 1 },
        ..ArmyLayout::default()
    };
    if count == 0 || !(rect.w > 0.0) || !(rect.h > 0.0) {
        return out;
    }
    let pad = (PANEL_PAD * s).min(rect.w.min(rect.h) * 0.25);
    let inner = rect.inset(pad, pad);
    if !(inner.w > 0.0) || !(inner.h > 0.0) {
        return out;
    }
    let visible = (CHIP_UNIT * s).max(touch_side).min(inner.h);
    if !(visible > 0.0) {
        return out;
    }
    let gap = (visible * GAP_RATIO).max(touch_side - visible).max(0.0);
    let advance = visible + gap;
    let one_page = fit_slots(inner.w, advance, gap);
    if one_page == 0 {
        return out;
    }

    // Two-pass pagination: full row first, then one slot reserved for the
    // page button so the counters are stable (no circular dependency).
    let (per_page, pages) = if count <= one_page {
        (count, 1usize)
    } else if one_page >= 2 {
        let per = one_page - 1;
        (per, (count + per - 1) / per)
    } else {
        (one_page, (count + one_page - 1) / one_page)
    };
    let pages = pages.max(1);
    let page = page.min(pages - 1);
    let first = page * per_page;
    let shown = count.saturating_sub(first).min(per_page);
    let top = inner.y + (inner.h - visible) * 0.5;
    let hit_side = visible.max(touch_side);
    let mut x = inner.x;
    for slot in 0..shown {
        let art = Rect::new(x, top, visible, visible);
        out.slots.push(art);
        out.hits.push(Rect::new(
            art.x + (visible - hit_side) * 0.5,
            art.y + (visible - hit_side) * 0.5,
            hit_side,
            hit_side,
        ));
        x += advance;
        let _ = slot;
    }
    if pages > 1 && one_page >= 2 {
        // Right-aligned in the last slot of the row.
        let art = Rect::new(inner.right() - visible, top, visible, visible);
        let hit = Rect::new(
            art.x + (visible - hit_side) * 0.5,
            art.y + (visible - hit_side) * 0.5,
            hit_side,
            hit_side,
        );
        out.page_button = Some((art, hit));
    }
    out.page = page;
    out.pages = pages;
    out.per_page = per_page;
    out.first = first;
    out.visible = visible;
    out.advance = advance;
    out
}

/// The 1:1 portrait window of a chip: the frame inset by the lane's border.
pub fn chip_icon(skin: &Skin, chip: Rect) -> Rect {
    army::chip_icon_rect(skin, chip)
}

/// The HP bar of a chip: pinned inside the portrait window's bottom edge, so it
/// stays inside the chip at every size (the retail bar overlays the portrait).
pub fn chip_hp(skin: &Skin, chip: Rect, scale: f32) -> Rect {
    army::chip_hp_rect(skin, chip, sane_scale(scale))
}

/// The mana hairline of a caster chip: directly above the HP bar, inside the
/// portrait window. Returns a zero-height rect when the chip is too small to
/// keep a portrait face under both bars.
pub fn chip_mana(skin: &Skin, chip: Rect, scale: f32) -> Rect {
    let hp = chip_hp(skin, chip, scale);
    let icon = chip_icon(skin, chip);
    if !(hp.h > 0.0) || !(icon.h > 0.0) {
        return Rect::new(hp.x, hp.y, 0.0, 0.0);
    }
    let height = (hp.h * MANA_RATIO).min(icon.h * 0.12);
    if height <= 0.0 || icon.h < hp.h * 3.0 {
        return Rect::new(hp.x, hp.y, 0.0, 0.0);
    }
    Rect::new(hp.x, hp.y - height, hp.w, height)
}

/// The box the caller draws the page counter in: the page plate inset by a
/// quarter, so a two-glyph counter never touches the bevel.
pub fn page_label_rect(art: Rect) -> Rect {
    art.inset(art.w * 0.12, art.h * 0.18)
}

// ---------------------------------------------------------------------------
// Hit testing
// ---------------------------------------------------------------------------

/// Page slot under `point` (use [`ArmyParts::unit_index`] for the unit), or
/// `None` — the gaps between chips are dead space.
pub fn hit(parts: &ArmyParts, point: Vec2) -> Option<usize> {
    parts.hits.iter().position(|slot| slot.contains(point))
}

/// Page button under `point`.
pub fn hit_order(parts: &ArmyParts, point: Vec2) -> Option<usize> {
    parts.order_hits.iter().position(|slot| slot.contains(point))
}

/// Classify a pointer position inside the army panel.
pub fn classify(parts: &ArmyParts, point: Vec2) -> ArmyHit {
    if let Some(slot) = hit(parts, point) {
        return ArmyHit::Chip(slot);
    }
    if let Some(slot) = hit_order(parts, point) {
        return ArmyHit::Page(slot);
    }
    ArmyHit::None
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

/// Label of tab `index` in `lang`, or `""` out of range.
pub fn tab_label(lang: TabLang, index: usize) -> &'static str {
    match lang {
        TabLang::Ru => TAB_LABELS_RU.get(index).copied().unwrap_or(""),
        TabLang::En => TAB_LABELS_EN.get(index).copied().unwrap_or(""),
    }
}

/// Keyboard shortcut of tab `index` for the desktop tooltip (`""` when the
/// phone has no such key path — the button never shows it as the way in).
pub fn tab_key_label(index: usize) -> &'static str {
    TAB_KEYS.get(index).copied().unwrap_or("")
}

/// Tooltip text for a keyboard user: `"КВЕСТЫ · F9"`, or the bare label when
/// the entry has no key.
pub fn tab_tooltip(lang: TabLang, index: usize) -> String {
    let label = tab_label(lang, index);
    let key = tab_key_label(index);
    if label.is_empty() {
        String::new()
    } else if key.is_empty() {
        label.to_string()
    } else {
        format!("{label} · {key}")
    }
}

/// What tab `index` opens.
pub fn tab_command(index: usize) -> Option<TabCommand> {
    match index {
        TAB_QUESTS => Some(TabCommand::QuestLog),
        TAB_MENU => Some(TabCommand::GameMenu),
        TAB_ALLIES => Some(TabCommand::Allies),
        TAB_JOURNAL => Some(TabCommand::Journal),
        _ => None,
    }
}

/// The `UiAction` a tab dispatches, when the action exists as a card action.
/// Quests: `UiAction::QuestLog`, exactly what F1/F9 activates.
pub fn tab_action(index: usize) -> Option<UiAction> {
    match tab_command(index)? {
        TabCommand::QuestLog => Some(UiAction::QuestLog),
        _ => None,
    }
}

/// Whether the tab has a working screen behind it. Disabled tabs are drawn dim
/// and must not swallow a tap (`Allies`/`Journal`: no screen exists yet).
pub fn tab_enabled(index: usize) -> bool {
    matches!(
        tab_command(index),
        Some(TabCommand::QuestLog) | Some(TabCommand::GameMenu)
    )
}

/// Tab row width for `touch` (`0` when a tab has no positive side).
pub fn tabs_width(touch: TouchMetrics, scale: f32) -> f32 {
    let s = sane_scale(scale);
    let side = touch.min_side();
    if !(side > 0.0) {
        return 0.0;
    }
    TAB_COUNT as f32 * side + (TAB_COUNT as f32 - 1.0) * TAB_GAP * s
}

/// Whether four 48 dp tabs fit inside `safe_frame` at all.
pub fn tabs_fit(safe_frame: Rect, touch: TouchMetrics, scale: f32) -> bool {
    tabs_width(touch, scale) <= safe_frame.w && touch.min_side() <= safe_frame.h
}

/// Touch rects of the four tabs, left to right, inside `safe_frame`.
///
/// A tab is a square `48 dp` tile (the framed-slot art, 102x99, is near-square
/// and upscales 1.24x without distortion). The four tiles start at the safe
/// frame's top-left; when the row does not fit, the gaps collapse to 0 and the
/// row reports itself oversized through [`tabs_fit`] rather than shrinking a
/// target below 48 dp.
pub fn tab_rects(safe_frame: Rect, touch: TouchMetrics, scale: f32) -> [Rect; TAB_COUNT] {
    let s = sane_scale(scale);
    let side = touch.min_side();
    let empty = [Rect::new(safe_frame.x, safe_frame.y, 0.0, 0.0); TAB_COUNT];
    if !(safe_frame.w > 0.0) || !(safe_frame.h > 0.0) || !(side > 0.0) {
        return empty;
    }
    let side = side.min(safe_frame.h);
    let gap = (TAB_GAP * s).min(((safe_frame.w - TAB_COUNT as f32 * side) / 3.0).max(0.0));
    let mut out = empty;
    let mut x = safe_frame.x;
    for tab in out.iter_mut() {
        *tab = Rect::new(x, safe_frame.y, side, side);
        x += side + gap;
    }
    out
}

/// The box the caller draws a tab label in: the tile inset by 8 %, so a
/// six-letter word (КВЕСТЫ, ЖУРНАЛ) fits a 126 px tile at the HUD's text scale.
pub fn tab_label_rect(tab: Rect) -> Rect {
    tab.inset(tab.w * 0.08, tab.h * 0.08)
}

/// Tab index under `point`; the gaps between tiles return `None`.
pub fn tab_hit(tabs: &[Rect; TAB_COUNT], point: Vec2) -> Option<usize> {
    tabs.iter().position(|tab| tab.contains(point))
}

// ---------------------------------------------------------------------------
// Skin sub-slices (`strip_top`, `button_normal` part0/part1)
// ---------------------------------------------------------------------------

/// UV rect of an authored sub-band (`dy..dy+height`) of `role`, with the same
/// parent-size guard `assets.rs::dialog_sub_slice_skin` uses: another atlas
/// keeps its parent element instead of sampling a crop that is not there.
fn sub_slice_uv(
    skin: &Skin,
    role: &str,
    expect: [f32; 2],
    dy: f32,
    height: f32,
) -> Option<[f32; 4]> {
    let index = Skin::role_index(role)?;
    if !(skin.width > 0.0) || !(skin.height > 0.0) {
        return None;
    }
    let [ax, ay, aw, ah] = skin.rects[index];
    let full = [
        ax / skin.width,
        ay / skin.height,
        (ax + aw) / skin.width,
        (ay + ah) / skin.height,
    ];
    if (aw - expect[0]).abs() > SUBSLICE_TOLERANCE
        || (ah - expect[1]).abs() > SUBSLICE_TOLERANCE
        || !(height > 0.0)
        || dy < 0.0
        || dy + height > ah
    {
        return Some(full);
    }
    Some([
        ax / skin.width,
        (ay + dy) / skin.height,
        (ax + aw) / skin.width,
        (ay + dy + height) / skin.height,
    ])
}

/// One quad of an authored plate (the crop as-is, stretched to the target).
fn draw_crop(
    skin: &Skin,
    batch: &mut UiBatch,
    role: &str,
    crop: ([f32; 2], f32, f32),
    rect: Rect,
    tint: [f32; 4],
) {
    if !(rect.w > 0.0) || !(rect.h > 0.0) {
        return;
    }
    match sub_slice_uv(skin, role, crop.0, crop.1, crop.2) {
        Some(uv) => batch.textured_quad(skin.texture, rect, uv, tint),
        None => batch.border(rect, (LINE_H).max(1.0), tint),
    }
}

// ---------------------------------------------------------------------------
// Draw
// ---------------------------------------------------------------------------

/// Draw the army panel for `page` and return every emitted rect.
///
/// See [`layout`] for the geometry rules. `selected` is the **absolute** unit
/// index (the primary subgroup member); it is highlighted wherever it lands.
#[allow(clippy::too_many_arguments)]
pub fn draw_page(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    units: &[ArmyChip<'_>],
    selected: usize,
    page: usize,
    scale: f32,
    touch: TouchMetrics,
    tint: [f32; 4],
) -> ArmyParts {
    let s = sane_scale(scale);
    let laid = layout(rect, units.len(), page, s, touch);
    let mut parts = ArmyParts {
        frame: rect,
        page: laid.page,
        pages: laid.pages,
        first: laid.first,
        ..ArmyParts::default()
    };
    if !(rect.w > 0.0) || !(rect.h > 0.0) {
        return parts;
    }

    // Plate + closing hairline. No atlas role: a solid plate, never a hole.
    let plate = mul(FRAME_TINT, tint);
    if !skin.nine(batch, ROLE_PANEL, rect, plate) {
        batch.rect(rect, plate);
    }
    let line_h = (LINE_H * s).min(rect.h * 0.25);
    if line_h > 0.0 {
        batch.rect(
            Rect::new(rect.x, rect.bottom() - line_h, rect.w, line_h),
            mul(FRAME_LINE, tint),
        );
    }

    for (slot, chip) in laid.slots.iter().copied().enumerate() {
        let Some(unit) = units.get(laid.first + slot) else {
            break;
        };
        debug_assert!(
            !unit.def.is_empty(),
            "army chip {} must carry its unit def id",
            laid.first + slot
        );
        parts.chips.push(chip);
        parts.hits.push(laid.hits[slot]);
        let is_primary = laid.first + slot == selected;

        // Portrait well under the frame: the caller paints the icon texture
        // here; without an icon key the well reads as an empty plate.
        let icon = chip_icon(skin, chip);
        if icon.w > 0.0 && icon.h > 0.0 {
            let well = if unit.icon.is_some() {
                ICON_WELL
            } else {
                ICON_WELL_EMPTY
            };
            batch.rect(icon, mul(well, tint));
        }

        // Frame over the well and the portrait.
        let chip_tint = if is_primary {
            mul(tint, SELECT_TINT)
        } else {
            tint
        };
        let role = if is_primary {
            army::ROLE
        } else {
            army::ROLE_DARK
        };
        if !skin.nine(batch, role, chip, chip_tint) {
            batch.border(chip, LINE_H.max(1.0), chip_tint);
        }

        // HP: track + fill, red at zero, green at full.
        let hp = chip_hp(skin, chip, s);
        if hp.w > 0.0 && hp.h > 0.0 {
            batch.rect(hp, mul(army::HP_TRACK, tint));
            let ratio = unit.hp.clamp(0.0, 1.0);
            let fill = Rect::new(hp.x, hp.y, hp.w * ratio, hp.h);
            if fill.w > 0.0 {
                batch.rect(fill, mul(army::hp_color(ratio), tint));
            }
        }

        // Mana hairline for casters.
        if let Some(mana) = unit.mana {
            let bar = chip_mana(skin, chip, s);
            if bar.w > 0.0 && bar.h > 0.0 {
                batch.rect(bar, mul(MANA_TRACK, tint));
                let ratio = mana.clamp(0.0, 1.0);
                let fill = Rect::new(bar.x, bar.y, bar.w * ratio, bar.h);
                if fill.w > 0.0 {
                    batch.rect(fill, mul(MANA_FILL, tint));
                }
            }
        }

        // Primary marker along the frame's bottom edge.
        if is_primary {
            let mark_h = (MARK_H * s).min(chip.h * 0.12);
            let inset = (LINE_H * s).min(chip.w * 0.25);
            let mark_w = (chip.w - 2.0 * inset).max(0.0);
            if mark_h > 0.0 && mark_w > 0.0 {
                batch.rect(
                    Rect::new(
                        chip.x + inset,
                        chip.bottom() - mark_h - inset,
                        mark_w,
                        mark_h,
                    ),
                    mul(SELECT_MARK, tint),
                );
            }
        }
    }

    if let Some((art, hit)) = laid.page_button {
        parts.order_buttons.push(art);
        parts.order_hits.push(hit);
        // Page plate: the near-square slot frame at the touch size.
        if !skin.nine(batch, ROLE_TAB, art, tint) {
            batch.border(art, LINE_H.max(1.0), tint);
        }
    }

    parts
}

/// [`draw_page`] with page 0 and the device touch metrics (the spec entry
/// point; the wiring uses `draw_page` with the caller's page state).
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    units: &[ArmyChip<'_>],
    selected: usize,
    scale: f32,
    tint: [f32; 4],
) -> ArmyParts {
    draw_page(
        skin,
        batch,
        rect,
        units,
        selected,
        0,
        scale,
        TouchMetrics::DEVICE,
        tint,
    )
}

/// Draw the four retail top-bar tabs at the left of `safe_frame` and return
/// their touch rects.
///
/// Tiles are square `48 dp` framed slots ([`ROLE_TAB`]); the active tile uses
/// the gold frame ([`ROLE_TAB_ACTIVE`]); a disabled tab (no screen behind it) is
/// subdued so it never reads as a live button. The `strip_top` band is drawn
/// under the row at its authored 27 px height — the bar the retail tabs attach
/// to. Labels are the caller's font draw ([`tab_label`]).
pub fn draw_top_tabs(
    skin: &Skin,
    batch: &mut UiBatch,
    safe_frame: Rect,
    scale: f32,
    active: Option<usize>,
    lang: TabLang,
    tint: [f32; 4],
) -> [Rect; TAB_COUNT] {
    let s = sane_scale(scale);
    let tabs = tab_rects(safe_frame, TouchMetrics::DEVICE, s);
    let first = tabs[0];
    if !(first.w > 0.0) || !(first.h > 0.0) {
        return tabs;
    }
    let _ = lang;
    // The retail bar band under the tiles (`strip_top`, authored 27 px).
    let band = Rect::new(
        first.x,
        first.bottom(),
        tabs[TAB_COUNT - 1].right() - first.x,
        (STRIP_TOP_H * s).min(safe_frame.bottom() - first.bottom()),
    );
    draw_crop(
        skin,
        batch,
        ROLE_PANEL,
        (STRIP_TOP_PARENT, STRIP_TOP_DY, STRIP_TOP_H),
        band,
        tint,
    );
    for (index, tab) in tabs.iter().copied().enumerate() {
        let (role, tile_tint) = if Some(index) == active {
            (ROLE_TAB_ACTIVE, tint)
        } else if tab_enabled(index) {
            (ROLE_TAB, mul(tint, DISABLED_NONE))
        } else {
            (ROLE_TAB, mul(tint, TAB_DISABLED_TINT))
        };
        if !skin.nine(batch, role, tab, tile_tint) {
            batch.border(tab, LINE_H.max(1.0), tile_tint);
        }
    }
    tabs
}

/// Tint multiplier for a tab that is enabled and inactive.
pub const DISABLED_NONE: [f32; 4] = [1.0; 4];
/// Dim multiplier for a tab with no screen behind it.
pub const TAB_DISABLED_TINT: [f32; 4] = [0.45, 0.45, 0.5, 0.7];

// ---------------------------------------------------------------------------
// Tests (GPU-free: synthetic skin fixture, real `UiBatch` vertex records)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    const EPS: f32 = 1e-2;
    /// HUD scale observed on the operator's device at 2414x1080.
    const S: f32 = 1.1;
    /// Measured `MobileHudLayout::army` on 2414x1080 (probe, 2026-11-15).
    const ARMY_LANDSCAPE: Rect = Rect {
        x: 499.28003,
        y: 914.9895,
        w: 490.05005,
        h: 103.41046,
    };
    /// Measured `MobileHudLayout::army` on 1080x2414 (probe, 2026-11-15).
    const ARMY_PORTRAIT: Rect = Rect {
        x: 249.96352,
        y: 2298.8672,
        w: 234.91661,
        h: 77.16626,
    };
    /// Measured `MobileHudLayout::safe_frame` on 2414x1080.
    const SAFE_LANDSCAPE: Rect = Rect {
        x: 52.800003,
        y: 88.0,
        w: 2308.4,
        h: 939.2,
    };
    /// Measured `MobileHudLayout::safe_frame` on 1080x2414.
    const SAFE_PORTRAIT: Rect = Rect {
        x: 12.655441,
        y: 50.621765,
        w: 1054.6892,
        h: 2329.6304,
    };
    /// Recommended band for the row: selection column to the command card,
    /// 126 px tall (one 48 dp chip).
    const BAND_LANDSCAPE: Rect = Rect {
        x: 1040.0,
        y: 890.0,
        w: 940.0,
        h: 126.0,
    };
    const BAND_PORTRAIT: Rect = Rect {
        x: 12.6,
        y: 2170.0,
        w: 1054.7,
        h: 126.0,
    };

    fn chips(count: usize, mana_every: usize) -> Vec<ArmyChip<'static>> {
        (0..count)
            .map(|index| ArmyChip {
                icon: if index % 3 == 2 { None } else { Some("BTNHero") },
                hp: 1.0 - index as f32 / (count.max(1) as f32 + 1.0),
                mana: if mana_every > 0 && index % mana_every == 0 {
                    Some(0.75)
                } else {
                    None
                },
                def: "hfoo",
            })
            .collect()
    }

    fn inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x - EPS
            && inner.y >= outer.y - EPS
            && inner.right() <= outer.right() + EPS
            && inner.bottom() <= outer.bottom() + EPS
    }

    /// A vertex colour is linear; the army fill constants are authored in sRGB.
    /// Classify a solid quad by the authored colour it came from.
    fn is_authored(linear: [f32; 4], authored: [f32; 4]) -> bool {
        crate::hud::authored_color(linear)
            .iter()
            .zip(authored.iter())
            .all(|(actual, expected)| (actual - expected).abs() <= 1e-6)
    }

    fn solid_quads(batch: &UiBatch) -> Vec<(Rect, [f32; 4])> {
        batch
            .white
            .chunks(6)
            .map(|verts| {
                let xs = verts.iter().map(|v| v.pos[0]);
                let ys = verts.iter().map(|v| v.pos[1]);
                let x0 = xs.clone().fold(f32::MAX, f32::min);
                let x1 = xs.fold(f32::MIN, f32::max);
                let y0 = ys.clone().fold(f32::MAX, f32::min);
                let y1 = ys.fold(f32::MIN, f32::max);
                (Rect::new(x0, y0, x1 - x0, y1 - y0), verts[0].color)
            })
            .collect()
    }

    fn first_uv_in(batch: &UiBatch, skin: &Skin, role: &str) -> Option<usize> {
        let index = Skin::role_index(role)?;
        let [ax, _, aw, _] = skin.rects[index];
        let (lo, hi) = (ax / skin.width - 1e-4, (ax + aw) / skin.width + 1e-4);
        batch
            .textured
            .iter()
            .flat_map(|draw| draw.vertices.iter())
            .position(|vertex| vertex.uv[0] >= lo && vertex.uv[0] <= hi)
    }

    /// Every interactive rect this module emits must reach the 48 dp target.
    fn assert_touch_sized(parts: &ArmyParts, touch: TouchMetrics) {
        let min = touch.min_side();
        for hit in &parts.hits {
            assert!(
                hit.w.min(hit.h) >= min - EPS,
                "chip hit {hit:?} is below the {min} px target"
            );
        }
        for hit in &parts.order_hits {
            assert!(
                hit.w.min(hit.h) >= min - EPS,
                "page hit {hit:?} is below the {min} px target"
            );
        }
    }

    fn assert_row_fits(rect: Rect, parts: &ArmyParts, touch: TouchMetrics) {
        assert!(!parts.chips.is_empty(), "a non-empty page must draw chips");
        for (slot, chip) in parts.chips.iter().enumerate() {
            assert!(inside(rect, *chip), "chip {chip:?} escapes the panel {rect:?}");
            assert!((chip.w - chip.h).abs() < EPS, "chips are 1:1: {chip:?}");
            // The art may be smaller than the finger target, never larger.
            let hit = parts.hits[slot];
            assert!(
                inside(hit, *chip) || (chip.w >= hit.w - EPS && chip.h >= hit.h - EPS),
                "chip {chip:?} and its hit {hit:?} disagree"
            );
        }
        for pair in parts.chips.windows(2) {
            assert!(
                pair[0].right() <= pair[1].x + EPS,
                "chips overlap: {:?} / {:?}",
                pair[0],
                pair[1]
            );
        }
        for pair in parts.hits.windows(2) {
            assert!(
                pair[0].right() <= pair[1].x + EPS,
                "touch rects overlap: {:?} / {:?}",
                pair[0],
                pair[1]
            );
        }
        for (index, button) in parts.order_buttons.iter().enumerate() {
            assert!(inside(rect, *button), "page button {button:?} escapes {rect:?}");
            let hit = parts.order_hits[index];
            for chip_hit in &parts.hits {
                assert!(
                    hit.right() <= chip_hit.x + EPS || hit.x >= chip_hit.right() - EPS,
                    "page hit overlaps a chip hit: {hit:?} / {chip_hit:?}"
                );
            }
        }
        assert_touch_sized(parts, touch);
    }

    #[test]
    fn army_row_paginates_instead_of_shrinking_below_48dp() {
        let skin = sample_skin();
        // Orientation, its real HUD scale, and the slots one page holds.
        for (band, scale, per_page, orientation) in [
            (BAND_LANDSCAPE, S, 6usize, "landscape"),
            (BAND_PORTRAIT, 0.5273, 6usize, "portrait"),
        ] {
            for count in [1usize, 5, 12] {
                let units = chips(count, 3);
                let mut batch = UiBatch::default();
                let parts = draw(&skin, &mut batch, band, &units, 0, scale, [1.0; 4]);
                assert_eq!(parts.first, 0);
                assert_eq!(
                    parts.chips.len(),
                    count.min(per_page),
                    "{orientation} n={count}: page slot count"
                );
                let pages = (count + per_page - 1) / per_page;
                assert_eq!(parts.pages, pages.max(1), "{orientation} n={count}: pages");
                assert_row_fits(band, &parts, TouchMetrics::DEVICE);
                let laid = layout(band, count, 0, scale, TouchMetrics::DEVICE);
                assert!((parts.chips[0].w - laid.visible).abs() < EPS);
                assert_eq!(laid.per_page, per_page.min(count));
            }
        }
    }

    #[test]
    fn measured_army_rect_still_yields_touch_targets_but_costs_pages() {
        // The panel the lane currently ships (490x103 landscape / 235x77
        // portrait) is below one 48 dp chip in height; the module keeps the
        // portrait inside the panel and pads the touch rect to 126 px.
        let laid = layout(ARMY_LANDSCAPE, 12, 0, S, TouchMetrics::DEVICE);
        assert!((laid.visible - (ARMY_LANDSCAPE.h - 2.0 * PANEL_PAD * S)).abs() < EPS);
        assert_eq!((laid.per_page, laid.pages), (3, 4));
        assert!(laid.page_button.is_some());
        assert!((laid.advance - TOUCH_MIN_PX).abs() < EPS, "advance clamps to the target");

        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, ARMY_LANDSCAPE, &chips(12, 3), 0, S, [1.0; 4]);
        assert_eq!((parts.chips.len(), parts.pages, parts.first), (3, 4, 0));
        assert_row_fits(ARMY_LANDSCAPE, &parts, TouchMetrics::DEVICE);

        // Portrait: one portrait + the page button per page — the arithmetic
        // the doc reports as "this rect cannot host 48 dp portraits".
        let portrait = layout(ARMY_PORTRAIT, 12, 0, 0.5273, TouchMetrics::DEVICE);
        assert_eq!((portrait.per_page, portrait.pages), (1, 12));
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, ARMY_PORTRAIT, &chips(12, 3), 0, 0.5273, [1.0; 4]);
        assert_eq!((parts.chips.len(), parts.pages), (1, 12));
        assert_row_fits(ARMY_PORTRAIT, &parts, TouchMetrics::DEVICE);
    }

    #[test]
    fn page_window_maps_slots_to_absolute_units() {
        let skin = sample_skin();
        let units = chips(12, 3);
        let mut page_two = UiBatch::default();
        let parts = draw_page(
            &skin,
            &mut page_two,
            BAND_LANDSCAPE,
            &units,
            7,
            1,
            S,
            TouchMetrics::DEVICE,
            [1.0; 4],
        );
        assert_eq!((parts.page, parts.pages, parts.first), (1, 2, 6));
        assert_eq!(parts.chips.len(), 6);
        assert_eq!(parts.unit_index(0), Some(6));
        assert_eq!(parts.unit_index(5), Some(11));
        assert_eq!(parts.unit_index(6), None);
        assert_eq!(parts.page_label(), "2/2");
        // Selected unit 7 is the second slot on page 1.
        let mut batch = UiBatch::default();
        let highlighted = draw_page(
            &skin,
            &mut batch,
            BAND_LANDSCAPE,
            &units,
            7,
            1,
            S,
            TouchMetrics::DEVICE,
            [1.0; 4],
        );
        let gold = first_uv_in(&batch, &skin, army::ROLE).expect("gold frame");
        let dark = first_uv_in(&batch, &skin, army::ROLE_DARK).expect("dark frame");
        assert!(dark < gold, "slot 0 is dark, the primary slot 1 is gold");
        assert_eq!(highlighted.chips.len(), 6);
        // A page past the end clamps instead of drawing nothing.
        let mut batch = UiBatch::default();
        let clamped = draw_page(
            &skin,
            &mut batch,
            BAND_LANDSCAPE,
            &units,
            0,
            99,
            S,
            TouchMetrics::DEVICE,
            [1.0; 4],
        );
        assert_eq!(clamped.page, clamped.pages - 1);
        assert_eq!(clamped.first, 6);
        // One page: no counter, no page button.
        let mut batch = UiBatch::default();
        let small = draw(&skin, &mut batch, BAND_LANDSCAPE, &chips(3, 0), 0, S, [1.0; 4]);
        assert_eq!(small.page_label(), "");
        assert!(small.order_buttons.is_empty());
    }

    #[test]
    fn hp_bar_stays_inside_the_chip_and_follows_hp() {
        let skin = sample_skin();
        let units = chips(5, 2);
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, BAND_LANDSCAPE, &units, 0, S, [1.0; 4]);
        let mut widths = Vec::new();
        for (index, chip) in parts.chips.iter().enumerate() {
            let bar = chip_hp(&skin, *chip, S);
            assert!(inside(*chip, bar), "hp {bar:?} escapes chip {chip:?}");
            let icon = chip_icon(&skin, *chip);
            assert!(inside(icon, bar), "hp {bar:?} escapes the portrait {icon:?}");
            assert!((bar.bottom() - icon.bottom()).abs() < EPS);
            assert!(bar.h <= icon.h * 0.25 + EPS);
            widths.push(bar.w * units[index].hp.clamp(0.0, 1.0));
        }
        assert!(
            widths.windows(2).all(|w| w[0] >= w[1] - EPS),
            "fills must follow hp: {widths:?}"
        );
    }

    #[test]
    fn mana_bar_only_for_casters_and_inside_the_icon() {
        let skin = sample_skin();
        let units = vec![
            ArmyChip {
                icon: Some("BTNHero"),
                hp: 1.0,
                mana: Some(0.5),
                def: "Hpal",
            },
            ArmyChip {
                icon: Some("BTNFootman"),
                hp: 1.0,
                mana: None,
                def: "hfoo",
            },
        ];
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, BAND_LANDSCAPE, &units, 0, S, [1.0; 4]);
        let caster = parts.chips[0];
        let mana = chip_mana(&skin, caster, S);
        let hp = chip_hp(&skin, caster, S);
        let icon = chip_icon(&skin, caster);
        assert!(mana.h > 0.0, "a caster gets the mana hairline");
        assert!(inside(icon, mana), "mana {mana:?} escapes the portrait {icon:?}");
        assert!((mana.bottom() - hp.y).abs() < EPS, "mana sits on the hp bar");
        assert!(mana.h < hp.h, "mana is thinner than hp");
        // Exactly one mana fill is emitted: the caster, not the footman.
        let fills = solid_quads(&batch)
            .into_iter()
            .filter(|(_, color)| is_authored(*color, MANA_FILL))
            .count();
        assert_eq!(fills, 1, "one mana fill for one caster");
        assert_eq!(chip_hp(&skin, parts.chips[1], S).w > 0.0, true);
    }

    /// `ui.colour.army-mana-fill` — the caster mana hairline is the authored
    /// [`MANA_FILL`] written as a linear vertex colour, tracks the mana ratio on
    /// the bar, and stays inside the portrait; a non-caster emits no fill.
    #[test]
    fn ui_colour_army_mana_fill_is_authored_and_tracks_the_ratio() {
        let skin = sample_skin();
        let units = vec![
            ArmyChip {
                icon: Some("BTNHero"),
                hp: 1.0,
                mana: Some(0.5),
                def: "Hpal",
            },
            ArmyChip {
                icon: Some("BTNHero"),
                hp: 1.0,
                mana: Some(1.0),
                def: "Arch",
            },
            ArmyChip {
                icon: Some("BTNFootman"),
                hp: 1.0,
                mana: None,
                def: "hfoo",
            },
        ];
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, BAND_LANDSCAPE, &units, 0, S, [1.0; 4]);
        let fills: Vec<(Rect, [f32; 4])> = solid_quads(&batch)
            .into_iter()
            .filter(|(_, color)| is_authored(*color, MANA_FILL))
            .collect();
        assert_eq!(fills.len(), 2, "two casters, no fill for the footman");
        for (index, (fill, color)) in fills.iter().enumerate() {
            let bar = chip_mana(&skin, parts.chips[index], S);
            assert!(bar.h > 0.0, "caster {index} has a mana bar");
            assert!(inside(chip_icon(&skin, parts.chips[index]), *fill));
            let ratio = units[index].mana.expect("caster");
            assert!(
                (fill.w - bar.w * ratio).abs() <= 1.0,
                "caster {index}: fill {} != bar {} * {ratio}",
                fill.w,
                bar.w
            );
            assert!(
                (fill.h - bar.h).abs() < 0.01,
                "the fill is the bar's height: {} vs {}",
                fill.h,
                bar.h
            );
            assert_eq!(color[3], 1.0, "the mana fill is opaque");
        }
        for channel in 0..3 {
            assert!(
                (fills[0].1[channel] - crate::hud::srgb_to_linear(MANA_FILL[channel])).abs() < 1e-6,
                "channel {channel} is not linear MANA_FILL"
            );
        }
    }

    #[test]
    fn primary_chip_uses_the_gold_frame_and_a_marker() {
        let skin = sample_skin();
        let units = chips(5, 2);

        let mut first = UiBatch::default();
        draw(&skin, &mut first, BAND_LANDSCAPE, &units, 0, S, [1.0; 4]);
        let gold = first_uv_in(&first, &skin, army::ROLE).expect("gold frame");
        let dark = first_uv_in(&first, &skin, army::ROLE_DARK).expect("dark frame");
        assert!(gold < dark, "chip 0 is the primary: gold before dark");

        let mut mid = UiBatch::default();
        let parts = draw(&skin, &mut mid, BAND_LANDSCAPE, &units, 2, S, [1.0; 4]);
        let gold_mid = first_uv_in(&mid, &skin, army::ROLE).expect("gold frame");
        let dark_mid = first_uv_in(&mid, &skin, army::ROLE_DARK).expect("dark frame");
        assert!(dark_mid < gold_mid, "with selected=2 the dark frames come first");

        // Exactly one marker quad, on the selected chip's bottom edge.
        let marks: Vec<Rect> = solid_quads(&mid)
            .into_iter()
            .map(|(rect, _)| rect)
            .filter(|quad| {
                parts.chips.iter().any(|chip| {
                    quad.w > chip.w * 0.9
                        && quad.h > 0.0
                        && quad.h < chip.h * 0.2
                        && quad.y > chip.y + chip.h * 0.8
                        && quad.y < chip.bottom()
                })
            })
            .collect();
        assert_eq!(marks.len(), 1, "one primary marker: {marks:?}");

        // An out-of-range selection is clamped, never a panic.
        let mut batch = UiBatch::default();
        let parts_clamped = draw(&skin, &mut batch, BAND_LANDSCAPE, &units, 99, S, [1.0; 4]);
        assert_eq!(parts_clamped.chips.len(), parts.chips.len());
    }

    #[test]
    fn hit_test_finds_every_chip_centre_and_misses_the_gaps() {
        let skin = sample_skin();
        for count in [1usize, 5, 12] {
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, BAND_LANDSCAPE, &chips(count, 3), 0, S, [1.0; 4]);
            for (slot, slot_rect) in parts.hits.iter().enumerate() {
                assert_eq!(hit(&parts, slot_rect.centre()), Some(slot));
                assert_eq!(classify(&parts, slot_rect.centre()), ArmyHit::Chip(slot));
                assert_eq!(hit(&parts, Vec2::new(slot_rect.x, slot_rect.y)), Some(slot));
                assert_eq!(parts.unit_index(slot), Some(slot));
            }
            if parts.hits.len() > 1 {
                let a = parts.hits[0];
                let b = parts.hits[1];
                let gap_x = (a.right() + b.x) * 0.5;
                if b.x > a.right() + EPS {
                    assert!(
                        hit(&parts, Vec2::new(gap_x, a.y + a.h * 0.5)).is_none(),
                        "the gap between chips is dead space"
                    );
                }
            }
            assert_eq!(hit(&parts, Vec2::new(BAND_LANDSCAPE.right() + 50.0, BAND_LANDSCAPE.y)), None);
            assert_eq!(classify(&parts, Vec2::new(-10.0, -10.0)), ArmyHit::None);
            if let Some(page) = parts.order_hits.first() {
                assert_eq!(classify(&parts, page.centre()), ArmyHit::Page(0));
                assert_eq!(parts.order_buttons.len(), 1);
            }
        }
    }

    #[test]
    fn zero_units_draws_only_the_frame() {
        let skin = sample_skin();
        let rect = BAND_LANDSCAPE;
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, rect, &[], 0, S, [1.0; 4]);
        assert_eq!(parts.frame, rect);
        assert!(parts.chips.is_empty());
        assert!(parts.hits.is_empty());
        assert!(parts.order_buttons.is_empty());
        assert!(parts.order_hits.is_empty());
        assert_eq!(parts.pages, 0);
        assert_eq!(hit(&parts, rect.centre()), None);
        // Plate only: the nine-patch (9 quads) and the bottom hairline (1 quad).
        assert_eq!(batch.textured.len(), 1);
        assert_eq!(batch.textured[0].vertices.len(), 9 * 6);
        assert_eq!(batch.white.len(), 6);
        let (line, _) = solid_quads(&batch)[0];
        assert!((line.h - LINE_H * S).abs() < EPS);
        assert!((line.bottom() - rect.bottom()).abs() < EPS);
    }

    #[test]
    fn tabs_are_touch_sized_inside_the_safe_frame_and_ordered() {
        let skin = sample_skin();
        for (safe, scale) in [
            (SAFE_LANDSCAPE, S),
            (SAFE_PORTRAIT, 0.5273),
        ] {
            assert!(tabs_fit(safe, TouchMetrics::DEVICE, scale), "tabs must fit {safe:?}");
            let tabs = tab_rects(safe, TouchMetrics::DEVICE, scale);
            for tab in tabs {
                assert!(inside(safe, tab), "tab {tab:?} escapes {safe:?}");
                assert!(
                    tab.w.min(tab.h) >= TOUCH_MIN_PX - EPS,
                    "tab {tab:?} is below 48 dp"
                );
                assert!((tab.w - tab.h).abs() < EPS, "tiles are square");
                assert!((tab.y - safe.y).abs() < EPS, "tiles sit on the bar");
            }
            for pair in tabs.windows(2) {
                assert!(pair[0].right() <= pair[1].x + EPS, "tiles overlap: {pair:?}");
                assert!(pair[0].x < pair[1].x, "tiles are ordered");
            }
            assert_eq!(tabs[0].x, safe.x);
            assert!((tabs[3].right() - tabs[0].x - tabs_width(TouchMetrics::DEVICE, scale)).abs() < EPS);
            // The drawn row returns the same rects and every tile is a quad.
            let mut batch = UiBatch::default();
            let drawn = draw_top_tabs(&skin, &mut batch, safe, scale, Some(TAB_QUESTS), TabLang::Ru, [1.0; 4]);
            assert_eq!(drawn, tabs);
            assert!(!batch.textured.is_empty());
        }
    }

    #[test]
    fn tab_hit_maps_each_tab_and_the_gaps() {
        let tabs = tab_rects(SAFE_LANDSCAPE, TouchMetrics::DEVICE, S);
        for (index, tab) in tabs.iter().enumerate() {
            assert_eq!(tab_hit(&tabs, tab.centre()), Some(index));
            assert_eq!(tab_hit(&tabs, Vec2::new(tab.x, tab.y)), Some(index));
        }
        let gap = (tabs[0].right() + tabs[1].x) * 0.5;
        assert_eq!(tab_hit(&tabs, Vec2::new(gap, tabs[0].y + 1.0)), None);
        assert_eq!(tab_hit(&tabs, Vec2::new(SAFE_LANDSCAPE.right() - 1.0, SAFE_LANDSCAPE.y)), None);
    }

    #[test]
    fn tabs_are_touch_buttons_not_key_hints() {
        // Labels are words; the F-key is tooltip text only.
        assert_eq!(tab_label(TabLang::Ru, TAB_QUESTS), "КВЕСТЫ");
        assert_eq!(tab_label(TabLang::Ru, TAB_MENU), "МЕНЮ");
        assert_eq!(tab_label(TabLang::Ru, TAB_ALLIES), "АЛЬЯНС");
        assert_eq!(tab_label(TabLang::Ru, TAB_JOURNAL), "ЖУРНАЛ");
        assert_eq!(tab_label(TabLang::En, TAB_QUESTS), "QUESTS");
        assert_eq!(tab_label(TabLang::En, 4), "");
        for index in 0..TAB_COUNT {
            assert!(
                !tab_label(TabLang::Ru, index).contains("F"),
                "a phone label never carries a key: {:?}",
                tab_label(TabLang::Ru, index)
            );
            assert!(!tab_label(TabLang::En, index).contains("F1") && !tab_label(TabLang::En, index).contains("F9"));
        }
        assert_eq!(tab_tooltip(TabLang::Ru, TAB_QUESTS), "КВЕСТЫ · F9");
        assert_eq!(tab_tooltip(TabLang::En, TAB_MENU), "MENU · F10");
        assert_eq!(tab_tooltip(TabLang::Ru, TAB_ALLIES), "АЛЬЯНС");
        assert_eq!(tab_key_label(TAB_JOURNAL), "F12");
        assert_eq!(tab_key_label(TAB_ALLIES), "");

        // Both paths stay one implementation: the Quests tile dispatches the
        // same `UiAction::QuestLog` the F1/F9 handler activates.
        assert_eq!(tab_command(TAB_QUESTS), Some(TabCommand::QuestLog));
        assert_eq!(tab_action(TAB_QUESTS), Some(UiAction::QuestLog));
        assert_eq!(tab_command(TAB_MENU), Some(TabCommand::GameMenu));
        assert_eq!(tab_command(TAB_ALLIES), Some(TabCommand::Allies));
        assert_eq!(tab_command(TAB_JOURNAL), Some(TabCommand::Journal));
        assert_eq!(tab_command(4), None);
        assert!(tab_enabled(TAB_QUESTS));
        assert!(tab_enabled(TAB_MENU));
        assert!(!tab_enabled(TAB_ALLIES), "no ally screen yet: drawn disabled");
        assert!(!tab_enabled(TAB_JOURNAL), "no journal screen yet: drawn disabled");
    }

    #[test]
    fn draw_top_tabs_emits_the_band_and_four_tiles() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let tabs = draw_top_tabs(&skin, &mut batch, SAFE_LANDSCAPE, S, Some(TAB_MENU), TabLang::Ru, [1.0; 4]);
        assert_eq!(batch.textured.len(), 1, "one atlas draw");
        // Band + four tiles; the band is one quad, a tile is a nine-patch
        // (9 quads) on the fixture skin.
        assert_eq!(batch.textured[0].vertices.len(), (1 + 4 * 9) * 6);

        // The band samples the authored `strip_top` crop of `wide_panel`.
        let wide = Skin::role_index(ROLE_PANEL).unwrap();
        let [ax, ay, aw, _ah] = skin.rects[wide];
        let strip_v0 = (ay + STRIP_TOP_DY) / skin.height;
        let strip_v1 = (ay + STRIP_TOP_DY + STRIP_TOP_H) / skin.height;
        let band = &batch.textured[0].vertices[0..6];
        let (u0, u1) = (band[0].uv[0], band[1].uv[0]);
        let (v0, v1) = (band[0].uv[1], band[2].uv[1]);
        assert!((u0 - ax / skin.width).abs() < 1e-5);
        assert!((u1 - (ax + aw) / skin.width).abs() < 1e-5);
        assert!((v0 - strip_v0).abs() < 1e-5, "band starts at strip_top");
        assert!((v1 - strip_v1).abs() < 1e-5, "band is 27 px tall");
        assert!(
            (v1 - v0 - STRIP_TOP_H / skin.height).abs() < 1e-5,
            "authored 27 px band"
        );

        // The active tile uses the gold role, the rest the dark role.
        assert!(first_uv_in(&batch, &skin, ROLE_TAB_ACTIVE).is_some());
        let gold = first_uv_in(&batch, &skin, ROLE_TAB_ACTIVE).unwrap();
        let dark = first_uv_in(&batch, &skin, ROLE_TAB).unwrap();
        assert!(dark < gold, "tab 0 (dark) precedes the active tab 1 (gold)");
        let _ = tabs;
    }

    #[test]
    fn every_interactive_rect_is_at_least_48dp_at_2414x1080() {
        let skin = sample_skin();
        let mut interactive: Vec<(&'static str, Rect)> = Vec::new();
        for (name, rect, count, selected) in [
            ("measured landscape", ARMY_LANDSCAPE, 12usize, 0usize),
            ("recommended landscape", BAND_LANDSCAPE, 12, 5),
            ("portrait", BAND_PORTRAIT, 12, 11),
            ("one unit", BAND_LANDSCAPE, 1, 0),
        ] {
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, rect, &chips(count, 3), selected, S, [1.0; 4]);
            for hit in &parts.hits {
                interactive.push((name, *hit));
            }
            for hit in &parts.order_hits {
                interactive.push((name, *hit));
            }
            // The page counter is drawn inside the page plate.
            for art in &parts.order_buttons {
                let label = page_label_rect(*art);
                assert!(inside(*art, label), "page label {label:?} escapes {art:?}");
            }
        }
        let tabs = tab_rects(SAFE_LANDSCAPE, TouchMetrics::DEVICE, S);
        // The real HUD font decides whether the word fits the tile: every RU
        // label must be drawable at >= MIN_TEXT_PX inside its label box.
        let font = crate::hud::Font::load();
        for (index, tab) in tabs.iter().enumerate() {
            interactive.push(("tab", *tab));
            let label = tab_label_rect(*tab);
            assert!(inside(*tab, label), "tab label {label:?} escapes {tab:?}");
            for lang in [TabLang::Ru, TabLang::En] {
                let text = tab_label(lang, index);
                let by_width = label.w / font.measure(text, 1.0);
                let by_height = label.h * 0.6 / font.line_height;
                let scale = by_width.min(by_height);
                let px = scale * font.line_height;
                assert!(
                    px >= crate::hud::MIN_TEXT_PX,
                    "{text}: {px:.1} px is below the readable floor"
                );
                assert!(font.measure(text, scale) <= label.w + EPS);
            }
        }
        assert!(!interactive.is_empty());
        for (name, rect) in interactive {
            assert!(
                rect.w.min(rect.h) >= TOUCH_MIN_PX - EPS,
                "{name}: {rect:?} is below 48 dp ({} px)",
                rect.w.min(rect.h)
            );
        }
        assert!(tabs_fit(SAFE_LANDSCAPE, TouchMetrics::DEVICE, S));
        assert!(tabs_fit(SAFE_PORTRAIT, TouchMetrics::DEVICE, 0.5273));
        assert!(inside(SAFE_LANDSCAPE, Rect::new(tabs[0].x, tabs[0].y, tabs[3].right() - tabs[0].x, tabs[0].h)));
    }

    #[test]
    fn tiny_panels_and_bad_scales_draw_the_frame_without_panics() {
        let skin = sample_skin();
        for rect in [
            Rect::new(0.0, 0.0, 0.0, 0.0),
            Rect::new(0.0, 0.0, 6.0, 6.0),
            Rect::new(0.0, 0.0, 40.0, 2.0),
        ] {
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, rect, &chips(4, 0), 2, S, [1.0; 4]);
            assert!(parts.chips.iter().all(|chip| inside(rect, *chip)));
            assert!(parts.order_buttons.is_empty());
        }
        // Non-finite scale never panics and still lands inside the panel.
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, BAND_LANDSCAPE, &chips(3, 0), 0, f32::NAN, [1.0; 4]);
        assert_row_fits(BAND_LANDSCAPE, &parts, TouchMetrics::DEVICE);
        assert_eq!(TouchMetrics::from_dp(48.0, 2.625).min_side(), TOUCH_MIN_PX);
        assert_eq!(TouchMetrics::from_dp(f32::NAN, 2.0).min_side(), TOUCH_MIN_PX);
    }
}
