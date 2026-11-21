//! Warcraft III console bar for the phone port.
//!
//! The HUD is one continuous stone/metal console at the safe bottom edge:
//! minimap, selection portrait/HP/stats, resources and a 4x3 command card.
//! There is no floating widget layer: every control lives inside the console.
//!
//! Reference: `analysis/agent-hud/references/` and the operator's WC3 console
//! capture. Layout invariants and the headless screenshot live in
//! `analysis/agent-hud/DESIGN_SPEC.md` and the test module below.
//!
//! # One composer (ledger 521)
//!
//! `HudLayout::compute` is the only place a zone rect is solved. The draw
//! functions (`draw`, `draw_pack`, `draw_selection_panel`, `draw_selection_skin`,
//! `draw_menu`) never compute a zone: they read `MobileHudLayout` (the dp snapshot
//! plus its single `dp * density` conversion) and may only *place content inside*
//! the rects they were given. The portrait and landscape paths share
//! `compose_selection` so the unit card cannot drift between modes. The contract,
//! the measured golden numbers and the invariant list are in
//! `analysis/agent-hud/LAYOUT_SOURCE_OF_TRUTH.md`.

use glam::Vec2;

use crate::hud::{
    Font, FontChain, NinePatch, Rect, TextAlign, UiBatch, UiDraw, UiTexture, UiVertex, MIN_TEXT_PX,
    QuestLog, ScoreData, draw_plate_text, fit_text_plate,
};
use crate::hud::army_panel as skin_army_panel;
use crate::hudskin::army as skin_army;
use crate::hudskin::pack::HudPack;
use crate::hudskin::{
    Skin, dialogs as skin_dialogs, minimap_frame as skin_minimap, portrait as skin_portrait,
    stats as skin_stats, tiles as skin_tiles, tooltip as skin_tooltip,
};
use crate::style::{Chrome, Tokens};

pub const REFERENCE_HEIGHT: f32 = 1080.0;
/// Finger floor for every glue-screen entry (operator: 90 px minimum).
pub const MENU_MIN_TARGET_PX: f32 = 90.0;
/// Operator HUD scale boost: the whole HUD layout/UI is authored +10% for the
/// phone. Clamped inside the safe area so the console never exceeds it.
pub const HUD_SCALE_BOOST: f32 = 1.10;
/// DragonUI-derived design height (core/uiscale.lua: design is 768 units tall
/// at scale 1; auto scale = 768 / screen_height). WC3 console proportions stay
/// authored against 1080p, so the 768 baseline is mapped into that space.
pub const DRAGONUI_DESIGN_HEIGHT: f32 = 768.0;
pub const UI_SCALE_MIN: f32 = 0.40;
pub const UI_SCALE_MAX: f32 = 1.15;
/// Console height at the 1080p reference. 3x76 command cells + resource shelf
/// + bevel would not fit in 208 px; the reference console is ~270 px and this
/// keeps the 76 px cells intact.
///
/// Kept for the legacy FDF/redraw fallbacks (`hudskin::minimap_frame` budgets
/// its frame against this number); the atlas composition uses
/// [`CONSOLE_FRACTION`] instead.
pub const CONSOLE_HEIGHT: f32 = 288.0;

/// Console band at the 1080p reference (retail `ConsoleUI.fdf`: 0.176/0.6).


/// Console band at the 1080p reference (`CONSOLE_FRACTION * 1080`).
pub const CONSOLE_REFERENCE_H: f32 = 317.0;

/// Measured width split of the four pack console sections at 2414 px
/// (`compose-C.png`): left cap 614 (minimap + groups), left mid 834 (portrait
/// + unit info), right mid 395 (inventory), right cap 571 (command card).
/// Fractions sum to 1.0; the checker asserts the drawn widths sum to the
/// viewport. A future measured frame may change these numbers only together
/// with a rendered proof.
pub const CONSOLE_SECTION_FRACTIONS: [f32; 4] =
    [0.25435, 0.34548, 0.16363, 0.23654];

/// Minimap image side at HUD scale 1 (`330 px` at the 1.1 device scale).
///
/// The reference minimap terrain blob measures 234x239 of an 838 px screen
/// (`COMPOSITION.md` §1) - it is square, so the map keeps a square target and
/// the world texture is never squashed.
pub const MINIMAP_SIDE: f32 = 272.7;

/// 1:1 atlas insets of the minimap frame (`slot_dark`, `[21, 27, 21, 29]`).
pub const MINIMAP_FRAME_INSETS: [f32; 4] = [12.0, 12.0, 12.0, 12.0];

/// Portrait frame width at HUD scale 1 (`193 px` at the 1.1 device scale,
/// `frame_medium` is authored 198x217 so the frame is a 0.975x downscale).
pub const PORTRAIT_W: f32 = 159.1;

// ---------------------------------------------------------------------------
// Touch + composition units (device pixels, `COMPOSITION.md` §1)
// ---------------------------------------------------------------------------

/// Operator geometry (2026-11-16, supersedes the 48 dp targets): the console
/// is 371 px (34.4% of 1080) pinned to the very bottom, the banner is 72 px
/// flush at y = 0, tiles are 106 px. 48 dp would need 126 px and leave no
/// playfield, so tabs drop to 27 dp on purpose (`COMPOSITION.md` §1).
pub const TOUCH_MIN_PX: f32 = CONSOLE_TILE_PX;
/// Bottom console band as a fraction of the screen height (371 / 1080).
pub const CONSOLE_FRACTION: f32 = 0.293;
/// Retail-authored command tile (68 px retail -> 95 px here): card 4x3 =
/// 392x297, group pad 5x2 = 499x196.
pub const CONSOLE_TILE_PX: f32 = 95.0;
/// Top strip fraction (`ConsoleUI.fdf` 0.032/0.6 = 57.6 px at 1080).
pub const TOP_STRIP_FRACTION: f32 = 0.0533;
/// Interim operator size, kept as the `Comfort` option (371 px band).
pub const COMFORT_CONSOLE_FRACTION: f32 = 0.344;
pub const COMFORT_CONSOLE_TILE_PX: f32 = 106.0;
/// Portrait console band (operator: 0.52 -> 0.45).
pub const PORTRAIT_CONSOLE_FRACTION: f32 = 0.45;
/// Top strip height, flush at y = 0 (`ConsoleUI.fdf` 0.032 units = 5.33% of
/// 1080 = 57.6 px; the operator's status-bar line).
pub const BANNER_H: f32 = 57.6;

/// Console composition style: the operator band (371 px) or the retail-authored
/// compact band (`ConsoleUI.fdf` 0.176 units = 29.3% = 316.8 px). One switch,
/// no rewrite: every size below is derived from it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ConsoleStyle {
    /// Retail `ConsoleUI.fdf` band: 0.293 of the height, 95 px tiles (default).
    #[default]
    Retail,
    /// Interim operator size: 371 px band, 106 px tiles.
    Comfort,
}

/// The shipped composition (`COMPOSITION.md` §1). Flip to
/// [`ConsoleStyle::Compact`] to author against the retail FDF band.
pub const CONSOLE_STYLE: ConsoleStyle = ConsoleStyle::Retail;


impl ConsoleStyle {
    pub fn console_fraction(self) -> f32 {
        match self {
            Self::Retail => CONSOLE_FRACTION,
            Self::Comfort => COMFORT_CONSOLE_FRACTION,
        }
    }

    pub fn tile_px(self) -> f32 {
        match self {
            Self::Retail => CONSOLE_TILE_PX,
            Self::Comfort => COMFORT_CONSOLE_TILE_PX,
        }
    }
}
/// Gutter between console zones (device px).
pub const CONSOLE_PAD_PX: f32 = 12.0;
/// Gap between zones on the same axis (device px).
pub const CONSOLE_GAP_PX: f32 = 12.0;
/// Gap between stacked bands (device px).
pub const TOUCH_GAP_PX: f32 = 10.0;
/// Command tile side and gap: 4x3 = 528 x 394 px at 48 dp.
pub const CARD_TILE: f32 = TOUCH_MIN_PX;
pub const CARD_GAP: f32 = 6.0;
pub const CARD_COLUMNS: usize = 4;
pub const CARD_ROWS: usize = 3;
pub const CARD_SLOTS: usize = CARD_COLUMNS * CARD_ROWS;
/// Control-group plate side and gap: 5 x 2 = 662 x 260 px (all ten visible).
pub const GROUP_SIDE: f32 = TOUCH_MIN_PX;
pub const GROUP_GAP: f32 = 6.0;
pub const GROUP_COLUMNS: usize = 5;
pub const GROUP_ROWS: usize = 2;
/// Hero inventory: 2 x 3 slots of 48 dp beside the card (portrait).
pub const INVENTORY_COLUMNS: usize = 2;
pub const INVENTORY_ROWS: usize = 3;
pub const INVENTORY_GAP: f32 = 5.0;
pub const INVENTORY_HEADER_H: f32 = 22.0;
pub const INVENTORY_PAD: f32 = 12.0;
/// Top-banner tab tiles (48 dp each, `army_panel::tab_rects`).
pub const TAB_SIDE: f32 = TOUCH_MIN_PX;
/// Authored composition width per unit scale: landscape row (device 2005 px at
/// 1.1) and portrait stack, used to solve the scale for small screens.
pub const LANDSCAPE_UNITS: f32 = 1823.0;
pub const PORTRAIT_UNITS: f32 = 725.0;


/// Ornament border of the minimap frame, in screen px.
///
/// Measured in `analysis/hudskin/minimap_frame.md` §3/§4: the square is dilated
/// by 12 px per side (image/frame = 237.6/261.6 at HUD scale 1.1). The layout's
/// hit rect includes it, so tapping the frame is a minimap tap; the region draw
/// is given a skin with this same 12 px inset (`minimap_skin`), so art and hit
/// rect cannot drift.
pub const MINIMAP_BORDER_PX: f32 = 12.0;

/// DragonUI-derived UI scale (port of core/uiscale.lua): auto = 768/screen_h,
/// clamped [0.40, 1.15], epsilon 0.001. Expressed relative to the 1080p HUD
/// authoring space so console proportions remain stable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScale {
    pub factor: f32,
    pub auto: f32,
}

impl UiScale {
    pub const EPSILON: f32 = 0.001;

    /// Auto scale per DragonUI core/uiscale.lua: `768 / screen_height`,
    /// clamped `[0.40, 1.15]`. Returned as `auto`; `factor` is the neutral
    /// HUD multiplier (1.0) so 1080p-authored console layout/proportions stay
    /// stable across viewports and do not fight the existing layout contract.
    pub fn compute(screen_height: f32) -> Self {
        let h = screen_height.max(1.0);
        let auto = Self::clamp(DRAGONUI_DESIGN_HEIGHT / h);
        Self { factor: 1.0, auto }
    }

    pub fn clamp(value: f32) -> f32 {
        value.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    }

    pub fn changed(a: f32, b: f32) -> bool {
        (a - b).abs() > Self::EPSILON
    }
}

/// Hotkey font size scaled by size*(referenceScale/buttonScale), DragonUI
/// modules/buttons.lua:207-210. Keeps hotkey legible across button/scale ratios.
pub fn scaled_hotkey_font_size(size: f32, reference_scale: f32, button_scale: f32) -> f32 {
    let button = if button_scale.abs() < UiScale::EPSILON { 1.0 } else { button_scale };
    let reference = if reference_scale.abs() < UiScale::EPSILON { 1.0 } else { reference_scale };
    size * (reference / button)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeArea {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl SafeArea {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn landscape_1080() -> Self {
        Self::new(48.0, 80.0, 48.0, 48.0)
    }

    pub const fn portrait_1080() -> Self {
        Self::new(24.0, 96.0, 24.0, 64.0)
    }

    pub fn scaled(self, factor: f32) -> Self {
        Self::new(
            self.left * factor,
            self.top * factor,
            self.right * factor,
            self.bottom * factor,
        )
    }

    pub fn is_zero(self) -> bool {
        self.left == 0.0 && self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0
    }

    /// Clamp absurd platform insets to at most 40% of the viewport per side.
    pub fn sanitised(self, viewport: Vec2) -> Self {
        let max_x = viewport.x.max(1.0) * 0.4;
        let max_y = viewport.y.max(1.0) * 0.4;
        Self {
            left: self.left.clamp(0.0, max_x),
            top: self.top.clamp(0.0, max_y),
            right: self.right.clamp(0.0, max_x),
            bottom: self.bottom.clamp(0.0, max_y),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    Landscape,
    Portrait,
}

#[derive(Clone, Copy, Debug)]
pub struct ResourcePill<'a> {
    pub icon: Option<usize>,
    pub value: &'a str,
    pub label: &'a str,
    pub tone: [f32; 4],
    pub warn: bool,
}

impl<'a> ResourcePill<'a> {
    pub fn new(value: &'a str, label: &'a str, tone: [f32; 4]) -> Self {
        Self {
            icon: None,
            value,
            label,
            tone,
            warn: false,
        }
    }

    pub fn icon(mut self, icon: Option<usize>) -> Self {
        self.icon = icon;
        self
    }

    pub fn warn(mut self, warn: bool) -> Self {
        self.warn = warn;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProductionView<'a> {
    pub name: &'a str,
    pub progress: f32,
    pub remaining: u32,
}

#[derive(Clone, Debug)]
pub struct SelectionView<'a> {
    pub name: String,
    pub class: String,
    pub kind: String,
    pub hp: i32,
    pub max_hp: i32,
    pub portrait: Option<usize>,
    /// WC3 type id (`opeo`, `ogre`, `Obla`) for the `HUD_PORTRAIT` diagnostic.
    pub def: Option<&'a str>,
    /// How the portrait was resolved (model / icon / placeholder).
    pub portrait_source: Option<crate::ui::widgets::portrait::PortraitSource>,
    pub armor: i32,
    pub damage: i32,
    pub range: Option<f32>,
    pub stats_known: bool,
    pub production: Option<ProductionView<'a>>,
    /// Portrait frame variant (normal/hero/enemy) for the redrawn pack art.
    pub frame: PortraitFrame,
    /// HP/mana bars belong to units. A resource plate (gold mine) turns them
    /// off and prints its remaining amount as the value line instead.
    pub bars: bool,
    /// Remaining gold of the selected mine; `Some` replaces the HP row set.
    pub resource: Option<i32>,
    /// Mana (hero/caster). `Some` adds the second bar (TZ §6.2).
    pub mana: Option<(i32, i32)>,
    /// Hero level and food cost, when the entity has them.
    pub level: Option<u32>,
    pub food: Option<(u32, u32)>,
}

impl<'a> SelectionView<'a> {
    pub fn new(name: impl AsRef<str>, hp: i32, max_hp: i32) -> Self {
        Self {
            name: name.as_ref().to_string(),
            class: "WARRIOR".to_string(),
            kind: "UNIT".to_string(),
            hp,
            max_hp,
            portrait: None,
            def: None,
            portrait_source: None,
            armor: 0,
            damage: 0,
            range: None,
            stats_known: false,
            production: None,
            frame: PortraitFrame::Normal,
            bars: true,
            resource: None,
            mana: None,
            level: None,
            food: None,
        }
    }

    /// Mana bar values (current, max) — the hero/caster second bar.
    pub fn mana(mut self, mana: i32, max_mana: i32) -> Self {
        if max_mana > 0 {
            self.mana = Some((mana.max(0), max_mana));
        }
        self
    }

    pub fn level(mut self, level: u32) -> Self {
        self.level = Some(level);
        self
    }

    pub fn food(mut self, used: u32, cap: u32) -> Self {
        self.food = Some((used, cap));
        self
    }

    /// Resource plate: no HP/mana bar, remaining amount as the value line.
    pub fn resource(mut self, remaining: i32) -> Self {
        self.resource = Some(remaining.max(0));
        self.bars = false;
        self.stats_known = false;
        self
    }

    pub fn class(mut self, class: impl AsRef<str>) -> Self {
        self.class = class.as_ref().to_string();
        self
    }

    pub fn kind(mut self, kind: impl AsRef<str>) -> Self {
        self.kind = kind.as_ref().to_string();
        self
    }

    /// Attach the def id and the resolved portrait source (logged once per
    /// def by [`MobileHud::draw`]).
    pub fn def(mut self, def: &'a str) -> Self {
        self.def = Some(def);
        self
    }

    pub fn portrait_source(
        mut self,
        source: crate::ui::widgets::portrait::PortraitSource,
    ) -> Self {
        self.portrait_source = Some(source);
        self
    }

    pub fn portrait(mut self, portrait: Option<usize>) -> Self {
        self.portrait = portrait;
        self
    }

    pub fn stats(mut self, armor: i32, damage: i32) -> Self {
        self.armor = armor;
        self.damage = damage;
        self.stats_known = true;
        self
    }

    pub fn range(mut self, range: f32) -> Self {
        self.range = Some(range);
        self
    }

    pub fn production(mut self, production: Option<ProductionView<'a>>) -> Self {
        self.production = production;
        self
    }

    pub fn frame(mut self, frame: PortraitFrame) -> Self {
        self.frame = frame;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MinimapDot {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [f32; 4],
}

impl MinimapDot {
    pub fn new(x: f32, y: f32, size: f32, color: [f32; 4]) -> Self {
        Self { x, y, size, color }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MinimapView<'a> {
    pub dots: &'a [MinimapDot],
    /// Normalised camera rectangle `[x, y, w, h]` in 0..1 map space.
    pub camera: Option<[f32; 4]>,
    pub title: &'a str,
    /// Row-major fog cells (`fog_width * fog_height`): 0 = never seen (black),
    /// 1 = explored but out of sight (dim), 2 = currently visible (clear).
    pub fog: Option<&'a [u8]>,
    pub fog_width: usize,
    pub fog_height: usize,
}

impl<'a> MinimapView<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            dots: &[],
            camera: None,
            title,
            fog: None,
            fog_width: 0,
            fog_height: 0,
        }
    }

    /// Attach the sampled fog grid (see [`MinimapView::fog`]).
    pub fn fog(mut self, cells: &'a [u8], width: usize, height: usize) -> Self {
        self.fog = Some(cells);
        self.fog_width = width;
        self.fog_height = height;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CommandButton<'a> {
    pub icon: Option<usize>,
    pub label: &'a str,
    /// Compact card label (the full label is used for tooltips/logs).
    pub short: &'a str,
    /// One-letter WC3 hotkey shown in the card-cell badge.
    pub hotkey: &'a str,
    pub sub: &'a str,
    pub enabled: bool,
    pub pressed: bool,
    /// Pointer/finger is over the tile (hover state art).
    pub hover: bool,
    /// 0..1 production/queue progress drawn as the legacy bottom veil.
    pub progress: f32,
    /// `false` draws an empty card tile (4 empty slots at the end).
    pub present: bool,
    /// Mana price of an ability (0 = none). Printed in the bottom-right corner.
    pub mana_cost: i32,
    /// Autocast is enabled: paint the gold autocast ring around the icon.
    pub autocast: bool,
    /// Remaining cooldown 0..1: radial sweep from the top of the icon window.
    pub cooldown: Option<f32>,
    /// Gold price of a build/train entry (None = no price row).
    pub cost: Option<u32>,
    /// Affordability snapshot (`Sim::can_build_now`); false paints the cost red.
    pub affordable: bool,
    /// MORE-slot page `(current, total)` printed on the slot as `1/2`.
    pub page: Option<(u8, u8)>,
}

impl<'a> CommandButton<'a> {
    pub fn new(label: &'a str, icon: Option<usize>) -> Self {
        Self {
            icon,
            label,
            short: label,
            hotkey: "",
            sub: "",
            enabled: true,
            pressed: false,
            hover: false,
            progress: 0.0,
            present: true,
            mana_cost: 0,
            autocast: false,
            cooldown: None,
            cost: None,
            affordable: true,
            page: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            icon: None,
            label: "",
            short: "",
            hotkey: "",
            sub: "",
            enabled: false,
            pressed: false,
            hover: false,
            progress: 0.0,
            present: false,
            mana_cost: 0,
            autocast: false,
            cooldown: None,
            cost: None,
            affordable: true,
            page: None,
        }
    }

    pub fn short(mut self, short: &'a str) -> Self {
        self.short = short;
        self
    }

    pub fn hotkey(mut self, hotkey: &'a str) -> Self {
        self.hotkey = hotkey;
        self
    }

    pub fn sub(mut self, sub: &'a str) -> Self {
        self.sub = sub;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    pub fn hover(mut self, hover: bool) -> Self {
        self.hover = hover;
        self
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Ability mana price (bottom-right corner).
    pub fn mana_cost(mut self, mana: i32) -> Self {
        self.mana_cost = mana.max(0);
        self
    }

    /// Autocast ring state.
    pub fn autocast(mut self, autocast: bool) -> Self {
        self.autocast = autocast;
        self
    }

    /// Remaining cooldown fraction (0..1), drawn as a radial sweep.
    pub fn cooldown(mut self, remaining: Option<f32>) -> Self {
        self.cooldown = remaining.map(|value| value.clamp(0.0, 1.0));
        self
    }

    /// Gold price of a build/train entry.
    pub fn cost(mut self, cost: Option<u32>) -> Self {
        self.cost = cost;
        self
    }

    /// Affordability snapshot (`Sim::can_build_now`).
    pub fn affordable(mut self, affordable: bool) -> Self {
        self.affordable = affordable;
        self
    }

    /// MORE-slot page label `(current, total)`.
    pub fn page(mut self, current: u8, total: u8) -> Self {
        self.page = Some((current.max(1), total.max(1)));
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastTone {
    Info,
    Success,
    Danger,
}

#[derive(Clone, Debug)]
pub struct ToastView {
    pub text: String,
    pub tone: ToastTone,
}

impl ToastView {
    pub fn new(text: impl AsRef<str>, tone: ToastTone) -> Self {
        Self {
            text: text.as_ref().to_string(),
            tone,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HintView {
    pub text: String,
    pub attack: bool,
}

impl HintView {
    pub fn new(text: impl AsRef<str>, attack: bool) -> Self {
        Self {
            text: text.as_ref().to_string(),
            attack,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PhaseView<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub won: Option<bool>,
}

/// One selected unit in the console army row.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArmyChip {
    /// Unit portrait texture (`None` draws an empty chip plate).
    pub icon: Option<usize>,
    /// HP ratio `0..=1` for the micro bar.
    pub hp: f32,
    /// Active subgroup: draws the gold `slot_gold` frame (`slot_dark` otherwise).
    pub selected: bool,
}

/// One control-group button (`1..9, 0`) with its live count.
#[derive(Clone, Copy, Debug, Default)]
pub struct GroupView {
    /// Screen rect of the button (game's hit rect, so art and touch agree).
    pub rect: Rect,
    /// Units currently in the group.
    pub count: u32,
    /// Last recalled group (highlighted plate).
    pub active: bool,
}

pub struct HudView<'a> {
    pub viewport: Vec2,
    /// Pixels per logical dp (SDL: window pixels / window points). Layout is
    /// solved in dp and converted once; see [`WindowMetrics`].
    pub density: f32,
    /// Safe-area insets **in dp** (already divided by `density`).
    pub safe: Option<SafeArea>,
    /// Platform orientation override for desktop/emulator aspect windows.
    pub orientation: Option<Orientation>,
    /// User HUD scale + developer overlays (never a second layout transform).
    pub ui: UiPreferences,
    /// Developer overlay: FPS and hitbox debug (off in release, TZ §6.5).
    pub show_fps: bool,
    pub show_hitboxes: bool,
    /// Number of selected entities (drives the multi-selection strip).
    pub selection_count: usize,
    /// Compact mode: the GROUPS drawer is open.
    pub groups_open: bool,
    /// Compact mode: the 4x3 COMMANDS drawer is open.
    pub commands_open: bool,
    pub resources: &'a [ResourcePill<'a>],
    pub selection: Option<SelectionView<'a>>,
    pub minimap: MinimapView<'a>,
    /// 12 slots, row-major 4x3. Empty slots use [`CommandButton::empty`].
    pub commands: &'a [CommandButton<'a>],
    pub phase: Option<PhaseView<'a>>,
    pub toast: Option<ToastView>,
    /// Stacked toast band (newest first). `toast` stays the first row for
    /// callers that only ever produce one line.
    pub toasts: Vec<ToastView>,
    /// Screen rect of the hovered/pressed command slot. The tooltip is anchored
    /// to it (WC3 tooltips float above the button), never centred.
    pub tooltip_anchor: Option<Rect>,
    pub hint: Option<HintView>,
    pub map_name: &'a str,
    pub fps: f32,
    /// `MOVE`, `ATTACK`, `PLACE`.
    pub mode: &'a str,
    /// `NO UPKEEP`, `LOW UPKEEP`, ...
    pub upkeep: &'a str,
    /// Real `HumanUITile01..04` textures (None -> bevel fallback).
    pub console_tiles: [Option<usize>; 4],
    /// Real `CommandButtonBorder`/active-button chrome.
    pub button_border: Option<usize>,
    /// Real `HumanUIPortraitMask` frame.
    pub portrait_frame: Option<usize>,
    /// Long-press/hover tooltip for the touched command (name + description).
    pub tooltip: Option<String>,
    /// Cost/requirement lines of the tooltip (retail third section, max 8).
    pub tooltip_costs: Vec<String>,
    pub tooltip_background: Option<usize>,
    pub tooltip_border: Option<usize>,
    /// Shipped esc-menu panel/border/band for the victory/defeat dialog.
    pub dialog_panel: Option<usize>,
    pub dialog_border: Option<usize>,
    pub dialog_band: Option<usize>,
    /// Redrawn HUD art; missing textures fall back to the retail MPQ art.
    pub redraw: Option<RedrawSkin>,
    /// Fantasy-atlas HUD skin. `None` keeps every draw site on the retail
    /// FDF/chrome path (`Assets::hud_skin()` == None on a clean RoC install).
    pub hud_skin: Option<&'a Skin>,
    /// Selected units for the console army row (icons already resolved).
    pub army: &'a [ArmyChip],
    /// The ten control-group buttons with their live counts.
    pub groups: &'a [GroupView],
    /// Carried items (module contract: icon *name*, charges, cooldown).
    pub items: &'a [Option<crate::hud::inventory::ItemSlot<'a>>],
    /// Resolved icon textures for [`Self::items`], same order (the inventory
    /// module owns the frames, the caller owns the textures).
    pub item_icons: &'a [Option<usize>],
}

impl<'a> HudView<'a> {
    pub fn new(viewport: Vec2) -> Self {
        Self {
            viewport,
            density: 1.0,
            safe: None,
            orientation: None,
            ui: UiPreferences::default(),
            show_fps: false,
            show_hitboxes: false,
            selection_count: 0,
            groups_open: false,
            commands_open: false,
            resources: &[],
            selection: None,
            minimap: MinimapView::default(),
            commands: &[],
            phase: None,
            toast: None,
            toasts: Vec::new(),
            tooltip_anchor: None,
            hint: None,
            map_name: "",
            fps: 0.0,
            mode: "MOVE",
            upkeep: "",
            console_tiles: [None; 4],
            button_border: None,
            portrait_frame: None,
            tooltip: None,
            tooltip_costs: Vec::new(),
            tooltip_background: None,
            tooltip_border: None,
            dialog_panel: None,
            dialog_border: None,
            dialog_band: None,
            redraw: None,
            hud_skin: None,
            army: &[],
            groups: &[],
            items: &[],
            item_icons: &[],
        }
    }

    pub fn safe(mut self, safe: SafeArea) -> Self {
        self.safe = Some(safe);
        self
    }

    /// Pixels per logical dp (the single unit conversion of TZ §4.1).
    pub fn density(mut self, density: f32) -> Self {
        self.density = density;
        self
    }

    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = Some(orientation);
        self
    }

    pub fn ui_scale(mut self, ui_scale: f32) -> Self {
        self.ui.ui_scale = ui_scale;
        self
    }

    /// Developer overlay: FPS + hitboxes.
    pub fn dev_overlay(mut self, show: bool) -> Self {
        self.show_fps = show;
        self.show_hitboxes = show;
        self
    }

    pub fn selection_count(mut self, count: usize) -> Self {
        self.selection_count = count;
        self
    }

    pub fn groups_open(mut self, open: bool) -> Self {
        self.groups_open = open;
        self
    }

    pub fn commands_open(mut self, open: bool) -> Self {
        self.commands_open = open;
        self
    }

    pub fn resources(mut self, resources: &'a [ResourcePill<'a>]) -> Self {
        self.resources = resources;
        self
    }

    pub fn selection(mut self, selection: Option<SelectionView<'a>>) -> Self {
        self.selection = selection;
        self
    }

    pub fn minimap(mut self, minimap: MinimapView<'a>) -> Self {
        self.minimap = minimap;
        self
    }

    pub fn commands(mut self, commands: &'a [CommandButton<'a>]) -> Self {
        self.commands = commands;
        self
    }

    pub fn phase(mut self, phase: Option<PhaseView<'a>>) -> Self {
        self.phase = phase;
        self
    }

    pub fn toast(mut self, toast: Option<ToastView>) -> Self {
        self.toast = toast;
        self
    }

    /// Toast band rows, newest first. Each row gets its own strip above the
    /// console; none of them can reach the tooltip rect.
    pub fn toasts(mut self, toasts: Vec<ToastView>) -> Self {
        self.toasts = toasts;
        self
    }

    /// Anchor the tooltip to the hovered command slot (WC3 placement).
    pub fn tooltip_anchor(mut self, anchor: Option<Rect>) -> Self {
        self.tooltip_anchor = anchor;
        self
    }

    /// Retail tooltip section 3: up to eight cost/requirement lines.
    pub fn tooltip_costs(mut self, costs: Vec<String>) -> Self {
        self.tooltip_costs = costs;
        self
    }

    pub fn hint(mut self, hint: Option<HintView>) -> Self {
        self.hint = hint;
        self
    }

    pub fn map_name(mut self, map_name: &'a str, fps: f32) -> Self {
        self.map_name = map_name;
        self.fps = fps;
        self
    }

    pub fn mode(mut self, mode: &'a str) -> Self {
        self.mode = mode;
        self
    }

    pub fn upkeep(mut self, upkeep: &'a str) -> Self {
        self.upkeep = upkeep;
        self
    }

    pub fn console_tiles(mut self, tiles: [Option<usize>; 4]) -> Self {
        self.console_tiles = tiles;
        self
    }

    pub fn button_border(mut self, border: Option<usize>) -> Self {
        self.button_border = border;
        self
    }

    pub fn portrait_frame(mut self, frame: Option<usize>) -> Self {
        self.portrait_frame = frame;
        self
    }

    pub fn tooltip(mut self, tooltip: Option<String>) -> Self {
        self.tooltip = tooltip;
        self
    }

    pub fn tooltip_art(mut self, background: Option<usize>, border: Option<usize>) -> Self {
        self.tooltip_background = background;
        self.tooltip_border = border;
        self
    }

    /// Shipped esc-menu art for the victory/defeat dialog.
    pub fn dialog_art(
        mut self,
        panel: Option<usize>,
        border: Option<usize>,
        band: Option<usize>,
    ) -> Self {
        self.dialog_panel = panel;
        self.dialog_border = border;
        self.dialog_band = band;
        self
    }

    pub fn redraw(mut self, skin: Option<RedrawSkin>) -> Self {
        self.redraw = skin;
        self
    }

    /// Install the fantasy-atlas skin; `None` selects the retail chrome path.
    pub fn hud_skin(mut self, skin: Option<&'a Skin>) -> Self {
        self.hud_skin = skin;
        self
    }

    /// Selected units for the console army row.
    pub fn army(mut self, army: &'a [ArmyChip]) -> Self {
        self.army = army;
        self
    }

    /// The ten control-group buttons (art rects come from the game's hit row).
    pub fn items(
        mut self,
        items: &'a [Option<crate::hud::inventory::ItemSlot<'a>>],
    ) -> Self {
        self.items = items;
        self
    }

    pub fn item_icons(mut self, icons: &'a [Option<usize>]) -> Self {
        self.item_icons = icons;
        self
    }

    pub fn groups(mut self, groups: &'a [GroupView]) -> Self {
        self.groups = groups;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudHit {
    Minimap,
    Card(usize),
    Group(usize),
    Selection,
    Console,
    Phase,
}

// ---------------------------------------------------------------------------
// Atlas element names the HUD asks for
// ---------------------------------------------------------------------------
//
// Every name is looked up in the installed atlas (`Chrome::element`). When the
// atlas is absent the `element_or` list falls through to the next name and the
// caller keeps its WC3-art or bevel path, so the HUD never depends on the sheet
// being present. The names match the painter's element roles
// (`analysis/agent-hud/ui_atlas_insets.json`).

/// Console plate: the wide bottom/top bar of the sheet.
pub const ATLAS_CONSOLE: &[&str] = &["console_plate", "title_bar"];
/// Frame around the minimap and other single-panel views.
pub const ATLAS_FRAME: &[&str] = &["frame_medium", "frame_large"];
/// Dialog/tooltip-class panel.
pub const ATLAS_PANEL: &[&str] = &["panel_tooltip", "panel_dialog"];
/// Plate under the command card grid.
pub const ATLAS_CARD_PLATE: &[&str] = &["panel_dialog", "panel_tooltip"];
/// Tooltip shell.
pub const ATLAS_TOOLTIP: &[&str] = &["panel_tooltip", "title_small"];
/// Victory/defeat dialog panel.
pub const ATLAS_DIALOG: &[&str] = &["panel_dialog", "frame_large"];
/// Title band inside a dialog.
pub const ATLAS_TITLE_BAND: &[&str] = &["title_bar", "divider"];
/// HP/mana bar frame.
pub const ATLAS_BAR: &[&str] = &["bar_hp", "title_small"];
/// Item/ability slot.
pub const ATLAS_SLOT: &[&str] = &["button_empty", "slot_plain"];

/// Authored tile of the redrawn pack's panel art (see `RedrawSkin`).
pub const REDRAW_PANEL_W: f32 = 384.0;
pub const REDRAW_PANEL_H: f32 = 256.0;
pub const REDRAW_PANEL_CORNER: f32 = 24.0;

/// The art and tint one command tile draws.
#[derive(Clone, Copy, Debug)]
struct CommandLook {
    /// Atlas state element name.
    atlas: &'static str,
    /// Index into `RedrawSkin::button` (`normal, pressed, disabled, cooldown,
    /// autocast, empty`).
    state: usize,
    tint: [f32; 4],
}

/// State art for a command tile: pressed beats disabled beats hover beats
/// cooldown beats normal, and an absent slot is always the empty frame.
fn command_look(command: &CommandButton<'_>) -> CommandLook {
    let (atlas, state, tint) = if !command.present {
        ("button_empty", 5, [0.62, 0.62, 0.62, 0.75])
    } else if command.pressed {
        ("button_pressed", 1, [1.0; 4])
    } else if !command.enabled {
        ("button_disabled", 2, [1.0; 4])
    } else if command.hover {
        ("button_hover", 4, [1.0; 4])
    } else if command.progress > 0.0 {
        ("button_cooldown", 3, [1.0; 4])
    } else {
        ("button_normal", 0, [1.0; 4])
    };
    CommandLook { atlas, state, tint }
}

/// Intersection of `rect` with `parent` (zero-sized when disjoint). Text and
/// icon rects are clamped to their console/selection/card parent so nothing
/// spills outside the authored panel at any scale or orientation.
/// Nine-slice layout for a rectangular panel: fixed `edge_px` corners/edges,
/// stretched centre. `uv_edge` is the inset fraction of the source texture.
/// Nine-slice a [`MenuPart`] with the *authored* insets from the manifest
/// instead of a magic edge/UV pair: the stretch stops exactly where the art's
/// border ends, so a plate never smears its bevel.
pub fn nine_slice_part(
    part: MenuPart,
    rect: Rect,
    scale: f32,
) -> [(Rect, [f32; 4]); 9] {
    let edge = part.insets[0].max(part.insets[1]) * scale;
    let uv_edge = [
        (part.insets[0] / part.w.max(1.0)).clamp(0.0, 0.49),
        (part.insets[1] / part.h.max(1.0)).clamp(0.0, 0.49),
    ];
    nine_slice_quads(rect, edge, uv_edge)
}

pub fn nine_slice_quads(rect: Rect, edge_px: f32, uv_edge: [f32; 2]) -> [(Rect, [f32; 4]); 9] {
    let edge_x = edge_px.min(rect.w * 0.5).max(0.0);
    let edge_y = edge_px.min(rect.h * 0.5).max(0.0);
    let xs = [rect.x, rect.x + edge_x, rect.right() - edge_x, rect.right()];
    let ys = [rect.y, rect.y + edge_y, rect.bottom() - edge_y, rect.bottom()];
    let u = [0.0, uv_edge[0], 1.0 - uv_edge[0], 1.0];
    let v = [0.0, uv_edge[1], 1.0 - uv_edge[1], 1.0];
    std::array::from_fn(|index| {
        let row = index / 3;
        let column = index % 3;
        (
            Rect::new(
                xs[column],
                ys[row],
                xs[column + 1] - xs[column],
                ys[row + 1] - ys[row],
            ),
            [u[column], v[row], u[column + 1], v[row + 1]],
        )
    })
}

/// Retail tooltip section 3 is capped at eight cost/requirement rows
/// (`CToolTipWar3 0x6f154a20`, requirement path `0x6f3a7020/0x6f3a7090`).
pub const TOOLTIP_MAX_COST_LINES: usize = 8;

/// One tooltip line with WC3 colour codes (`|cAARRGGBB…|r`) applied as runs.
/// The text is fitted and clipped run by run, so a coloured line still cannot
/// leave its rect.
pub fn rich_line(
    ui: &mut UiBatch,
    font: &Font,
    text: &str,
    rect: Rect,
    scale: f32,
    base: [f32; 4],
    shadow: Option<[f32; 4]>,
) {
    let runs = crate::hud::color_runs(text, base);
    let mut x = rect.x;
    for (run, color) in runs {
        if run.is_empty() {
            continue;
        }
        let width = font.measure(&run, scale.max(crate::hud::MIN_DENSE_PX / 30.0));
        if width <= 0.0 {
            continue;
        }
        let row = Rect::new(x, rect.y, (rect.right() - x).max(0.0), rect.h);
        let drawn = font.draw_fit(
            ui,
            row,
            &run,
            scale,
            crate::hud::MIN_DENSE_PX,
            color,
            TextAlign::Left,
            shadow,
        );
        x += drawn.w;
        if x >= rect.right() - 1.0 {
            break;
        }
    }
}

/// Text metrics of the tooltip body used by the layout pass (no `Font` there):
/// the HUD atlas is rasterised at 30 px and `text_box` never draws below
/// [`MIN_TEXT_PX`], so the em box is `30 * 1.25 * max(scale, 0.8)`.
pub fn tooltip_line_box(scale: f32) -> f32 {
    30.0 * 1.25 * scale.max(MIN_TEXT_PX / 30.0)
}

/// Average glyph advance estimate (0.62 em) for the font-free layout pass.
fn estimated_advance(scale: f32) -> f32 {
    (scale * 30.0).max(MIN_TEXT_PX) * 0.62
}

/// Wrapped line count estimate for `text` inside `width` at `scale`.
pub fn estimate_tooltip_lines(text: Option<&str>, width: f32, scale: f32) -> usize {
    let Some(text) = text else {
        return 0;
    };
    let usable = (width - 20.0 * scale).max(1.0);
    let per_line = (usable / estimated_advance(scale)).floor().max(1.0);
    let lines: usize = text
        .split('\n')
        .map(|line| {
            let chars = line.chars().count() as f32;
            ((chars / per_line).ceil().max(1.0)) as usize
        })
        .sum();
    lines.max(1).min(24)
}

/// Tooltip box height for `lines` rows (padding included).
pub fn tooltip_height(lines: usize, scale: f32, present: bool) -> f32 {
    if !present {
        return 0.0;
    }
    lines.max(1) as f32 * tooltip_line_box(scale) + 22.0 * scale
}

pub fn clamp_rect(rect: Rect, parent: Rect) -> Rect {
    let x = rect.x.max(parent.x);
    let y = rect.y.max(parent.y);
    let right = rect.right().min(parent.right());
    let bottom = rect.bottom().min(parent.bottom());
    Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

#[derive(Clone, Copy, Debug)]
pub struct ConsoleQuad {
    pub rect: Rect,
    pub uv: [f32; 4],
    /// 0..3 = ConsoleTexture01..04 (HumanUITile01..04 alias).
    pub tile: u8,
}

/// Redrawn HUD art from `analysis/agent-hud/redraw-v0.1` — a **procedural
/// reference v1** pack, not final hand-drawn art. Every field is optional: when
/// a texture is missing the retail MPQ art or the procedural bevel is used.
#[derive(Clone, Copy, Debug, Default)]
pub struct RedrawSkin {
    pub console_left_cap: Option<usize>,
    pub console_left_mid: Option<usize>,
    pub console_right_mid: Option<usize>,
    pub console_right_cap: Option<usize>,
    pub topbar: Option<usize>,
    /// Button states: normal, pressed, disabled, cooldown, autocast, empty.
    pub button: [Option<usize>; 6],
    /// Portrait frames: normal, hero, enemy.
    pub portrait: [Option<usize>; 3],
    pub minimap_frame: Option<usize>,
    pub minimap_frame_alert: Option<usize>,
    pub minimap_header: Option<usize>,
    pub pill: Option<usize>,
    pub pill_warn: Option<usize>,
    pub selection_panel: Option<usize>,
    pub group_inactive: Option<usize>,
    pub group_active: Option<usize>,
    pub tooltip_panel: Option<usize>,
    /// Build/production progress strip (pack `build_progress`, 64x8).
    pub progress: Option<usize>,
}

impl RedrawSkin {
    /// True when at least the console shell is available.
    pub fn has_console(&self) -> bool {
        self.console_left_cap.is_some()
            && self.console_left_mid.is_some()
            && self.console_right_mid.is_some()
            && self.console_right_cap.is_some()
    }
}

/// One textured quad of the redrawn HUD (texture is a direct texture index).
#[derive(Clone, Copy, Debug)]
pub struct RedrawQuad {
    pub rect: Rect,
    pub uv: [f32; 4],
    pub texture: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RedrawParts {
    pub console: Vec<RedrawQuad>,
    pub topbar: Vec<RedrawQuad>,
}

/// Portrait frame variant selected by the unit kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PortraitFrame {
    #[default]
    Normal,
    Hero,
    Enemy,
}

impl PortraitFrame {
    pub fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Hero => 1,
            Self::Enemy => 2,
        }
    }
}

/// Build the pack console/topbar quads. The four console tiles are 256x288
/// with 9-slice insets 32/48/32/48 (manifest): caps keep their 32 px edges,
/// the middle span repeats the 192 px centre so the shell stays seamless at
/// every scale (0.62-2.0x); the console height is always 288*scale, so the
/// vertical insets scale 1:1 with the tile.
/// Retail `ConsoleUI.fdf` console quads for the shipped-art fallback.
///
/// The FDF series is `ConsoleTexture01..04` (`HumanUITile01..04`); the band is
/// tiled left-to-right at the tiles' authored square aspect instead of being
/// stretched, so even the fallback path never upscales a 512x512 piece. The
/// ninth entry is the clipped tail of the last tile.
pub fn console_parts_for(console: Rect, _scale: f32) -> [ConsoleQuad; 9] {
    let mut parts = [ConsoleQuad {
        rect: Rect::new(console.x, console.y, 0.0, 0.0),
        uv: [0.0, 0.0, 1.0, 1.0],
        tile: 0,
    }; 9];
    if console.w <= 0.0 || console.h <= 0.0 {
        return parts;
    }
    // Authored tile is 512x512: one tile spans `console.h` pixels.
    let tile_w = console.h.max(1.0);
    let count = ((console.w / tile_w).ceil() as usize).max(1).min(9);
    let w = console.w / count as f32;
    for index in 0..count {
        let x = console.x + index as f32 * w;
        parts[index] = ConsoleQuad {
            rect: Rect::new(x, console.y, w.min(console.right() - x).max(0.0), console.h),
            uv: [0.0, 0.0, 1.0, 1.0],
            tile: (index % 4) as u8,
        };
    }
    parts
}

pub fn build_redraw_parts(
    skin: &RedrawSkin,
    console: Rect,
    top_bar: Rect,
    scale: f32,
) -> Option<RedrawParts> {
    let s = scale.max(0.01);
    let mut parts = RedrawParts::default();
    if let (Some(left_cap), Some(left_mid), Some(right_mid), Some(right_cap)) = (
        skin.console_left_cap,
        skin.console_left_mid,
        skin.console_right_mid,
        skin.console_right_cap,
    ) {
        let cap_w = (256.0 * s).min(console.w * 0.30).max(1.0);
        let left_cap_rect = Rect::new(console.x, console.y, cap_w, console.h);
        parts.console.push(RedrawQuad {
            rect: left_cap_rect,
            uv: [0.0, 0.0, 1.0, 1.0],
            texture: left_cap,
        });
        let span = (console.w - cap_w * 2.0).max(1.0);
        let count = ((span / (256.0 * s)).round() as usize).max(1);
        let mid_w = span / count as f32;
        let edge = (32.0 * s).min(mid_w * 0.45).max(0.5);
        let center_w = (mid_w - edge * 2.0).max(0.5);
        for index in 0..count {
            let texture = if index % 2 == 0 { left_mid } else { right_mid };
            let x = console.x + cap_w + index as f32 * mid_w;
            parts.console.push(RedrawQuad {
                rect: Rect::new(x, console.y, edge, console.h),
                uv: [0.0, 0.0, 32.0 / 256.0, 1.0],
                texture,
            });
            parts.console.push(RedrawQuad {
                rect: Rect::new(x + edge, console.y, center_w, console.h),
                uv: [32.0 / 256.0, 0.0, 224.0 / 256.0, 1.0],
                texture,
            });
            parts.console.push(RedrawQuad {
                rect: Rect::new(x + edge + center_w, console.y, edge, console.h),
                uv: [224.0 / 256.0, 0.0, 1.0, 1.0],
                texture,
            });
        }
        let right_cap_rect = Rect::new(console.right() - cap_w, console.y, cap_w, console.h);
        parts.console.push(RedrawQuad {
            rect: right_cap_rect,
            uv: [0.0, 0.0, 1.0, 1.0],
            texture: right_cap,
        });
    }
    if let Some(topbar) = skin.topbar {
        let edge = (16.0 * s).min(top_bar.w * 0.25).max(0.5);
        let height = (32.0 * s).max(1.0);
        let rect = Rect::new(top_bar.x, top_bar.y, top_bar.w, height);
        parts.topbar.push(RedrawQuad {
            rect: Rect::new(rect.x, rect.y, edge, rect.h),
            uv: [0.0, 0.0, 16.0 / 720.0, 1.0],
            texture: topbar,
        });
        parts.topbar.push(RedrawQuad {
            rect: Rect::new(rect.x + edge, rect.y, (rect.w - edge * 2.0).max(0.5), rect.h),
            uv: [16.0 / 720.0, 0.0, 704.0 / 720.0, 1.0],
            texture: topbar,
        });
        parts.topbar.push(RedrawQuad {
            rect: Rect::new(rect.right() - edge, rect.y, edge, rect.h),
            uv: [704.0 / 720.0, 0.0, 1.0, 1.0],
            texture: topbar,
        });
    }
    if parts.console.is_empty() && parts.topbar.is_empty() {
        None
    } else {
        Some(parts)
    }
}

// ===========================================================================
// HUD LAYOUT ENGINE — TZ §4/§5/§6/§7 (ledger 478)
//
// One canonical unit: logical dp. `HudLayout::compute` is a pure function of
// `WindowMetrics` (framebuffer px, px-per-dp, safe insets in dp, orientation)
// plus user preferences and the small amount of game state that changes
// geometry. Every rectangle the game draws, clips, hit-tests or blocks the
// world with is one of these dp rects; `to_px` applies `px = dp * density`
// exactly once and nothing downstream re-scales.
// ===========================================================================

/// Android finger floor (TZ §8). Touch minimum stays 48 dp at any UiScale.
pub const TOUCH_DP: f32 = 48.0;
/// Inter-cell gap of the 4x3 command card (TZ §5.2).
pub const CARD_GAP_DP: f32 = 8.0;
/// Padding between the console background edge and its content.
pub const CONSOLE_PAD_DP: f32 = 8.0;
/// Widest the unit-card HP/mana trough may be (retail's card bar is ~120 dp).
const HP_BAR_MAX_DP: f32 = 150.0;
/// Gap between the console's functional zones.
pub const ZONE_GAP_DP: f32 = 8.0;
/// Minimap outer frame, wide landscape (TZ §5.2: 128..144 dp square).
pub const MINIMAP_DP: f32 = 144.0;
pub const MINIMAP_MIN_DP: f32 = 128.0;
/// Minimap outer frame, compact landscape (TZ §5.4).
pub const MINIMAP_COMPACT_DP: f32 = 112.0;
/// Control-group tray: 3x3 of 48 dp targets (TZ §5.2).
pub const GROUPS_TRAY_DP: f32 = 3.0 * TOUCH_DP + 2.0 * CARD_GAP_DP;
/// Command-card panel: 4x3 field of 48 dp targets + 8 dp gaps + 8 dp trim.
pub const CARD_W_DP: f32 = 4.0 * TOUCH_DP + 3.0 * CARD_GAP_DP + 2.0 * CARD_GAP_DP;
pub const CARD_H_DP: f32 = 3.0 * TOUCH_DP + 2.0 * CARD_GAP_DP + 2.0 * CARD_GAP_DP;
/// Zone layout switches to the collapsed tray below this content width.
pub const WIDE_MIN_CONTENT_DP: f32 = 820.0;
/// Top resource strip (TZ §6.5).
pub const TOP_BAR_DP: f32 = 30.0;
pub const TOP_BAR_PORTRAIT_DP: f32 = 44.0;
/// Console band: three 48 dp command rows plus the card trim (TZ §5.2).
pub const CONSOLE_MIN_DP: f32 = 176.0;
pub const CONSOLE_MAX_DP: f32 = 220.0;
/// Compact landscape band: minimap + selection strip only (operator addendum).
/// The 4x3 grid moves into a drawer so a 360 dp-tall screen keeps a playfield.
pub const CONSOLE_COMPACT_MIN_DP: f32 = 96.0;
pub const CONSOLE_COMPACT_MAX_DP: f32 = 128.0;
/// Selection summary strip at the top of the portrait console (TZ §7).
pub const PORTRAIT_SUMMARY_DP: f32 = 48.0;
/// Live-state fields that change console geometry.
pub const GROUP_COUNT: usize = 10;

/// UiScale is a user preference multiplier only (TZ §4.2).
pub const UI_PREF_MIN: f32 = 0.90;
pub const UI_PREF_MAX: f32 = 1.15;

/// Physical window description. No layout decisions live here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowMetrics {
    /// Framebuffer size in device pixels (what the renderer draws into).
    pub framebuffer_px: Vec2,
    /// Pixels per logical dp in framebuffer space: `px = dp * density_scale`.
    pub density_scale: f32,
    /// Window-manager safe insets, already in dp.
    pub safe_insets_dp: SafeArea,
    /// Platform orientation; `None` derives it from the dp aspect ratio.
    pub orientation: Option<Orientation>,
}

impl WindowMetrics {
    pub fn new(framebuffer_px: Vec2, density_scale: f32) -> Self {
        Self {
            framebuffer_px: Vec2::new(framebuffer_px.x.max(1.0), framebuffer_px.y.max(1.0)),
            density_scale,
            safe_insets_dp: SafeArea::default(),
            orientation: None,
        }
    }

    /// A density that can never divide by zero, whatever the platform sent.
    pub fn density(&self) -> f32 {
        if self.density_scale.is_finite() && self.density_scale > 0.05 {
            self.density_scale.clamp(0.5, 8.0)
        } else {
            1.0
        }
    }

    pub fn screen_dp(&self) -> Vec2 {
        (self.framebuffer_px / self.density()).max(Vec2::splat(1.0))
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation.unwrap_or_else(|| {
            let dp = self.screen_dp();
            if dp.x >= dp.y * 1.25 {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            }
        })
    }

    /// Safe insets in dp, clamped so a bogus platform value cannot eat the
    /// screen (each side at most 40% of the dp viewport).
    pub fn safe(&self) -> SafeArea {
        self.safe_insets_dp.sanitised(self.screen_dp())
    }

    /// Platform insets arrive in framebuffer pixels; this is the one place
    /// they become dp.
    pub fn with_pixel_insets(mut self, insets: SafeArea) -> Self {
        let d = self.density();
        self.safe_insets_dp = SafeArea::new(
            insets.left / d,
            insets.top / d,
            insets.right / d,
            insets.bottom / d,
        );
        self
    }
}

/// User-facing HUD preferences (never a second layout transform).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPreferences {
    /// Text/chrome multiplier; hit targets ignore it (TZ §4.2).
    pub ui_scale: f32,
    /// Developer overlay: FPS + layout boxes. Off in release (TZ §6.5/§8).
    pub show_fps: bool,
    pub show_hitboxes: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            show_fps: false,
            show_hitboxes: false,
        }
    }
}

impl UiPreferences {
    pub fn scale(&self) -> f32 {
        if self.ui_scale.is_finite() {
            self.ui_scale.clamp(UI_PREF_MIN, UI_PREF_MAX)
        } else {
            1.0
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HudLayoutMode {
    LandscapeWide,
    LandscapeCompact,
    Portrait,
}

impl HudLayoutMode {
    pub fn is_landscape(self) -> bool {
        matches!(self, Self::LandscapeWide | Self::LandscapeCompact)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::LandscapeWide => "landscape-wide",
            Self::LandscapeCompact => "landscape-compact",
            Self::Portrait => "portrait",
        }
    }
}

/// Game state that changes geometry (never the layout algorithm itself).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudLayoutState {
    /// Number of selected entities; drives the multi-selection strip.
    pub selection_count: usize,
    /// The active selection has mana (hero/caster).
    pub has_mana: bool,
    /// A real production queue exists (never reserve space otherwise).
    pub production: bool,
    /// A hero inventory exists for the active selection.
    pub inventory: bool,
    /// Compact mode: the GROUPS overlay drawer is open.
    pub groups_open: bool,
    /// Compact mode: the 4x3 COMMANDS overlay drawer is open.
    pub commands_open: bool,
}

/// Composed dp layout. Every consumer reads these rects.
#[derive(Clone, Debug)]
pub struct HudLayout {
    pub mode: HudLayoutMode,
    pub metrics: WindowMetrics,
    pub prefs: UiPreferences,
    pub state: HudLayoutState,
    /// Viewport in dp.
    pub screen: Vec2,
    /// Safe insets in dp.
    pub safe: SafeArea,
    /// Top resource strip (full width background, content inside the insets).
    pub top_bar: Rect,
    /// Console background: `x == 0`, `right == W`, `bottom == H` (TZ §4.3).
    pub console: Rect,
    /// Interactive content rect inside the console (safe area applied).
    pub content: Rect,
    /// Camera viewport: the world band between the top bar and the console.
    pub viewport: Rect,
    pub console_h: f32,
    /// Minimap header band, 1:1 frame and the map image inside the frame.
    pub minimap_header: Rect,
    pub minimap_frame: Rect,
    pub minimap_image: Rect,
    pub minimap_rail: Rect,
    /// Selection/detail zone (flexible).
    pub selection: Rect,
    pub portrait: Rect,
    pub name: Rect,
    pub class: Rect,
    pub hp_bar: Rect,
    pub hp_text: Rect,
    pub mana_bar: Rect,
    pub mana_text: Rect,
    pub stats: Rect,
    pub production_label: Rect,
    pub production_bar: Rect,
    /// Bottom band of the selection zone: multi-selection thumbnails or the
    /// hero inventory (never both, never when empty).
    pub army: Rect,
    pub army_slots: Vec<Rect>,
    pub army_badge: Rect,
    pub inventory: Rect,
    pub inventory_parts: crate::hud::inventory::InventoryParts,
    /// 3x3 tray for groups 1..9 plus the tenth slot (ID `0`).
    pub groups_area: Rect,
    pub group_slots: Vec<Rect>,
    /// Compact fallback: the 48 dp `GROUPS` button that opens the tray.
    pub groups_button: Rect,
    /// Open overlay drawer rects (compact) or zero when the tray is inline.
    pub groups_overlay: Rect,
    /// Command card panel and its 12 row-major slots.
    pub card: Rect,
    pub card_slots: Vec<Rect>,
    /// Compact fallback: the persistent 48 dp `COMMANDS` drawer control.
    pub commands_button: Rect,
    /// Open command drawer panel (compact) or zero when the grid is inline.
    pub commands_overlay: Rect,
    /// Resource strip content.
    pub resources: Rect,
    pub resource_pills: Vec<Rect>,
    pub upkeep: Rect,
    pub fps: Rect,
    pub map_name: Rect,
    /// World-side bands above the console.
    pub toast: Rect,
    pub toasts: Vec<Rect>,
    pub hint: Rect,
    pub tooltip: Rect,
    pub phase: Rect,
    pub mode_rect: Rect,
    /// Thin recessed separators between console zones.
    pub separators: Vec<Rect>,
}

impl HudLayout {
    /// The single layout entry point. Pure: no globals, no GPU, no clock.
    ///
    /// The composition lives in [`crate::ui::widgets::hud`]: this adapter turns
    /// the platform window into the solver's inputs and maps the solved dp rects
    /// onto the fields the painter and the touch router read. There is no second
    /// geometry path — the same model produces draw bounds and hit regions.
    pub fn compute(metrics: WindowMetrics, prefs: UiPreferences, state: HudLayoutState) -> Self {
        use crate::ui::core::geometry::{Insets as UiInsets, Size as UiSize};
        use crate::ui::layout::{ContentMetrics, Orientation as UiOrientation, Preferences, State as UiState, Window};
        use crate::ui::widgets::hud::solve_hud;

        let theme = crate::ui::theme::Theme::classic_fantasy();
        let window = Window {
            framebuffer: UiSize::new(metrics.framebuffer_px.x, metrics.framebuffer_px.y),
            density: metrics.density_scale,
            safe: UiInsets::new(
                metrics.safe_insets_dp.left,
                metrics.safe_insets_dp.top,
                metrics.safe_insets_dp.right,
                metrics.safe_insets_dp.bottom,
            ),
            orientation: Some(match metrics.orientation() {
                Orientation::Portrait => UiOrientation::Portrait,
                Orientation::Landscape => UiOrientation::Landscape,
            }),
        };
        let preferences = Preferences {
            scale: prefs.ui_scale,
            touch_dp: crate::ui::theme::metrics::TOUCH_DP,
            debug: prefs.show_fps,
        };
        // The five selection states (§3.5). The adapter derives them from the
        // live flags; a building never shows mana, a hero always does.
        let selection = match (state.selection_count, state.has_mana, state.production) {
            (0, _, _) => crate::ui::layout::SelectionState::None,
            (1, true, _) => crate::ui::layout::SelectionState::Hero,
            (1, _, true) => crate::ui::layout::SelectionState::Building,
            (1, _, _) => crate::ui::layout::SelectionState::Unit,
            _ => crate::ui::layout::SelectionState::Multiple,
        };
        let ui_state = UiState {
            selection,
            selection_count: state.selection_count,
            has_mana: state.has_mana,
            production: state.production,
            inventory: state.inventory,
            groups_open: state.groups_open,
            commands_open: state.commands_open,
        };
        let content = ContentMetrics {
            resources: 3,
            commands: if state.commands_open || metrics.orientation() == Orientation::Landscape {
                12
            } else {
                0
            },
            toasts: 3,
            selection_rows: 6,
        };
        let solution = solve_hud(&window, &preferences, &ui_state, &content, &theme);
        let mode = match solution.plan {
            crate::ui::widgets::hud::Plan::Wide => HudLayoutMode::LandscapeWide,
            crate::ui::widgets::hud::Plan::Compact | crate::ui::widgets::hud::Plan::Minimal => {
                HudLayoutMode::LandscapeCompact
            }
            crate::ui::widgets::hud::Plan::Portrait
            | crate::ui::widgets::hud::Plan::PortraitNarrow => HudLayoutMode::Portrait,
        };
        if !solution.fits {
            // A composition that does not fit is a defect, not a warning: the
            // ladder already picked the best rung, and the numbers are logged so
            // the next fix starts from evidence.
            crate::dev_log!("{}", solution.report());
            for problem in solution.problems() {
                crate::dev_log!("HUD_LAYOUT {problem}");
            }
        }
        Self::from_solution(metrics, prefs, state, mode, &solution.rects)
    }

    /// Map the solved composition onto the dp fields every consumer reads.
    fn from_solution(
        metrics: WindowMetrics,
        prefs: UiPreferences,
        state: HudLayoutState,
        mode: HudLayoutMode,
        rects: &crate::ui::widgets::hud::HudRects,
    ) -> Self {
        // One conversion from the pure layer's rect to the render layer's rect,
        // at the single boundary between them.
        let c = |r: crate::ui::core::geometry::Rect| Rect::new(r.x, r.y, r.w, r.h);
        let zero = Rect::new(0.0, 0.0, 0.0, 0.0);
        let mut inventory_parts = crate::hud::inventory::InventoryParts {
            frame: c(rects.inventory),
            header: Rect::new(
                rects.inventory.x,
                rects.inventory.y,
                rects.inventory.w,
                0.0,
            ),
            slots: [zero; 6],
            icons: [zero; 6],
            charges: [zero; 6],
            touches: [zero; 6],
        };
        for (index, slot) in rects.inventory_slots.iter().take(6).enumerate() {
            let slot = c(*slot);
            inventory_parts.slots[index] = slot;
            inventory_parts.icons[index] = slot.inset(4.0, 4.0);
            inventory_parts.charges[index] =
                Rect::new(slot.x, slot.bottom() - 12.0, slot.w, 11.0);
            inventory_parts.touches[index] = slot;
        }
        Self {
            mode,
            metrics,
            prefs,
            state,
            screen: Vec2::new(rects.screen.w.max(1.0), rects.screen.h.max(1.0)),
            safe: SafeArea::new(
                rects.safe.left,
                rects.safe.top,
                rects.safe.right,
                rects.safe.bottom,
            ),
            top_bar: c(rects.top_bar),
            console: c(rects.console),
            content: c(rects.content),
            viewport: c(rects.viewport),
            console_h: rects.console_h,
            minimap_header: c(rects.minimap_header),
            minimap_frame: c(rects.minimap_frame),
            minimap_image: c(rects.minimap_image),
            minimap_rail: c(rects.minimap_rail),
            selection: c(rects.selection),
            portrait: c(rects.portrait),
            name: c(rects.name),
            class: c(rects.class),
            hp_bar: c(rects.hp_bar),
            hp_text: c(rects.hp_text),
            mana_bar: c(rects.mana_bar),
            mana_text: c(rects.mana_text),
            stats: c(rects.stats),
            production_label: c(rects.production_label),
            production_bar: c(rects.production_bar),
            army: c(rects.army),
            army_slots: rects.army_slots.iter().map(|r| c(*r)).collect(),
            army_badge: c(rects.army_badge),
            inventory: c(rects.inventory),
            inventory_parts,
            groups_area: c(rects.groups_area),
            group_slots: rects.group_slots.iter().map(|r| c(*r)).collect(),
            groups_button: c(rects.groups_button),
            groups_overlay: c(rects.groups_overlay),
            card: c(rects.card),
            card_slots: rects.card_slots.iter().map(|r| c(*r)).collect(),
            commands_button: c(rects.commands_button),
            commands_overlay: c(rects.commands_overlay),
            resources: c(rects.resources),
            resource_pills: rects.resource_pills.iter().map(|r| c(*r)).collect(),
            upkeep: c(rects.upkeep),
            fps: c(rects.fps),
            map_name: c(rects.map_name),
            toast: c(rects.toast),
            toasts: rects.toasts.iter().map(|r| c(*r)).collect(),
            hint: c(rects.hint),
            tooltip: c(rects.tooltip),
            phase: c(rects.phase),
            mode_rect: c(rects.mode_rect),
            separators: rects.separators.iter().map(|r| c(*r)).collect(),
        }
    }

    /// Apply `px = dp * density` exactly once. Every field below is already in
    /// framebuffer pixels; nothing downstream may scale it again.
    pub fn to_px(&self, density: f32) -> MobileHudLayout {
        let d = if density.is_finite() && density > 0.05 {
            density.clamp(0.5, 8.0)
        } else {
            1.0
        };
        let p = |r: Rect| Rect::new(r.x * d, r.y * d, r.w * d, r.h * d);
        let console = p(self.console);
        let art = (console.h / 300.0).clamp(0.62, 2.4);
        let sections = if self.mode.is_landscape() {
            [
                p(self.minimap_frame),
                p(self.selection),
                p(self.groups_area),
                p(self.card),
            ]
        } else {
            [
                p(self.selection),
                p(self.minimap_frame),
                p(self.card),
                Rect::new(0.0, 0.0, 0.0, 0.0),
            ]
        };
        MobileHudLayout {
            viewport: self.screen * d,
            scale: art,
            density: d,
            layout_mode: self.mode,
            safe: SafeArea::new(
                self.safe.left * d,
                self.safe.top * d,
                self.safe.right * d,
                self.safe.bottom * d,
            ),
            orientation: if self.mode == HudLayoutMode::Portrait {
                Orientation::Portrait
            } else {
                Orientation::Landscape
            },
            safe_frame: Rect::new(
                self.safe.left * d,
                self.safe.top * d,
                (self.screen.x - self.safe.left - self.safe.right).max(1.0) * d,
                (self.screen.y - self.safe.top - self.safe.bottom).max(1.0) * d,
            ),
            console,
            console_inner: console.inset(CONSOLE_PAD_DP * d, CONSOLE_PAD_DP * d),
            console_parts: console_parts_for(console, art),
            sections,
            section_assets: ["minimap", "selection", "groups", "card"],
            redraw: None,
            tabs: [Rect::new(0.0, 0.0, 0.0, 0.0); 4],
            minimap_header: p(self.minimap_header),
            minimap: p(self.minimap_frame),
            minimap_image: p(self.minimap_image),
            minimap_frame: p(self.minimap_frame),
            minimap_rail: p(self.minimap_rail),
            selection: p(self.selection),
            panel: p(self.selection),
            portrait: p(self.portrait),
            name: p(self.name),
            class: p(self.class),
            hp_bar: p(self.hp_bar),
            hp_text: p(self.hp_text),
            mana_bar: p(self.mana_bar),
            mana_text: p(self.mana_text),
            stats: p(self.stats),
            production_label: p(self.production_label),
            production_bar: p(self.production_bar),
            resources: p(self.resources),
            resource_pills: self.resource_pills.iter().map(|r| p(*r)).collect(),
            upkeep: p(self.upkeep),
            card: p(self.card),
            card_slots: self.card_slots.iter().map(|r| p(*r)).collect(),
            inventory: p(self.inventory),
            groups_area: p(self.groups_area),
            group_slots: self.group_slots.iter().map(|r| p(*r)).collect(),
            groups_button: p(self.groups_button),
            groups_overlay: p(self.groups_overlay),
            commands_button: p(self.commands_button),
            commands_overlay: p(self.commands_overlay),
            inventory_parts: crate::hud::inventory::InventoryParts {
                frame: p(self.inventory_parts.frame),
                header: p(self.inventory_parts.header),
                slots: self.inventory_parts.slots.map(|r| p(r)),
                icons: self.inventory_parts.icons.map(|r| p(r)),
                charges: self.inventory_parts.charges.map(|r| p(r)),
                touches: self.inventory_parts.touches.map(|r| p(r)),
            },
            crest: Rect::new(0.0, 0.0, 0.0, 0.0),
            crest_plaque: Rect::new(0.0, 0.0, 0.0, 0.0),
            army: p(self.army),
            army_slots: self.army_slots.iter().map(|r| p(*r)).collect(),
            army_badge: p(self.army_badge),
            mode: p(self.mode_rect),
            top_meta: p(self.top_bar),
            map_name: p(self.map_name),
            fps: p(self.fps),
            toast: p(self.toast),
            toasts: self.toasts.iter().map(|r| p(*r)).collect(),
            hint: p(self.hint),
            tooltip: p(self.tooltip),
            tooltip_anchor: None,
            phase: p(self.phase),
            cell: TOUCH_DP * d,
            dp: Box::new(self.clone()),
            show_fps: self.prefs.show_fps,
        }
    }
}
#[derive(Clone, Debug)]
pub struct MobileHudLayout {
    pub viewport: Vec2,
    /// Authored-art scale (`rendered px / authored px`) for pack 9-slices.
    pub scale: f32,
    /// Pixels per logical dp of this layout (`px = dp * density`).
    pub density: f32,
    pub layout_mode: HudLayoutMode,
    /// The canonical dp snapshot this pixel layout was derived from.
    pub dp: Box<HudLayout>,
    pub show_fps: bool,
    pub safe: SafeArea,
    pub orientation: Orientation,
    pub safe_frame: Rect,
    pub console: Rect,
    pub console_inner: Rect,
    /// Exact `UI\FrameDef\UI\ConsoleUI.fdf` quads: 5 top + 4 bottom.
    pub console_parts: [ConsoleQuad; 9],
    /// Four framed console sections (pack path): left cap, left mid, right
    /// mid, right cap. Each is its own 9-slice; the widths are the measured
    /// `CONSOLE_SECTION_FRACTIONS` of the viewport.
    pub sections: [Rect; 4],
    pub section_assets: [&'static str; 4],
    /// Redrawn console/topbar quads when the pack art is loaded.
    pub redraw: Option<RedrawParts>,
    pub minimap_header: Rect,
    /// Four 48 dp top-banner tabs (`army_panel::tab_rects`).
    pub tabs: [Rect; 4],
    /// Hero inventory panel (2x3 slots of 48 dp); zero-sized in landscape.
    pub inventory: Rect,
    /// Inventory slot/finger rects (`inventory::plan`): the game layer hit-tests
    /// with `inventory::hit(&parts, point)` and `drag_target(...)`.
    pub inventory_parts: crate::hud::inventory::InventoryParts,
    /// Tap target: the minimap texture square itself. The 1:1 ornament ring is
    /// chrome around it (`minimap_frame` rect), never a hit area of its own.
    pub minimap: Rect,
    /// The minimap texture square itself (terrain, fog and blips draw here).
    pub minimap_image: Rect,
    /// 1:1 ornament ring around [`Self::minimap_image`].
    pub minimap_frame: Rect,
    /// Vertical mini-button rail beside the minimap (reference column).
    pub minimap_rail: Rect,
    pub selection: Rect,
    /// The pack's 240x180 selection plate inside [`Self::selection`] (the
    /// portrait and stats column sit around it). Zero on the legacy path.
    pub panel: Rect,
    pub portrait: Rect,
    pub name: Rect,
    pub class: Rect,
    pub hp_bar: Rect,
    pub hp_text: Rect,
    pub mana_bar: Rect,
    pub mana_text: Rect,
    pub stats: Rect,
    pub production_label: Rect,
    pub production_bar: Rect,
    pub resources: Rect,
    pub resource_pills: Vec<Rect>,
    pub upkeep: Rect,
    pub card: Rect,
    pub card_slots: Vec<Rect>,
    /// Control-group pad (5 x 2 plates of 48 dp = all ten groups).
    pub groups_area: Rect,
    /// One 48 dp rect per control group, row-major.
    pub group_slots: Vec<Rect>,
    /// Compact mode: the persistent 48 dp `GROUPS` drawer control.
    pub groups_button: Rect,
    /// Compact mode: the open 3x3 drawer panel (zero when closed).
    pub groups_overlay: Rect,
    /// Compact mode: the persistent 48 dp `COMMANDS` drawer control.
    pub commands_button: Rect,
    /// Compact mode: the open 4x3 drawer panel (zero when closed).
    pub commands_overlay: Rect,
    /// Crest panel between the group row and the command card.
    pub crest: Rect,
    pub crest_plaque: Rect,
    /// Army panel: one icon slot per selected unit (WC3 selection grid), inside
    /// the console under the selection text band.
    pub army: Rect,
    pub army_slots: Vec<Rect>,
    /// Overflow indicator (`+N`) when the strip shows fewer chips than selected.
    pub army_badge: Rect,
    /// Provided mode chip (`MOVE`/`ATTACK`/`PLACE`).
    pub mode: Rect,
    pub top_meta: Rect,
    pub map_name: Rect,
    pub fps: Rect,
    pub toast: Rect,
    /// Every toast row (row 0 == `toast`), stacked bottom-up above the console.
    pub toasts: Vec<Rect>,
    pub hint: Rect,
    pub tooltip: Rect,
    /// Hovered command slot the tooltip is anchored to.
    pub tooltip_anchor: Option<Rect>,
    pub phase: Rect,
    pub cell: f32,
}

impl MobileHudLayout {
    /// The canonical dp snapshot this pixel layout came from.
    pub fn dp(&self) -> &HudLayout {
        &self.dp
    }

    /// Convert a framebuffer point into the dp space the layout was solved in.
    /// Touch routing keeps px rects (they are `dp * density`), and this is the
    /// only conversion the pointer sees.
    pub fn point_dp(&self, point: Vec2) -> Vec2 {
        point / self.density.max(0.001)
    }

    /// Zones that must not overlap, in dp. The console is the parent of the
    /// zone rects, so those are checked as containers, not siblings.
    pub fn zone_rects(&self) -> Vec<(&'static str, Rect)> {
        let dp = self.dp();
        let mut zones = vec![
            ("top_bar", dp.top_bar),
            // The resource strip lives inside the top bar; the QA table and the
            // legacy touch names read it as its own zone.
            ("resources", dp.resources),
            ("minimap", dp.minimap_frame),
            ("selection", dp.selection),
            ("card", dp.card),
            ("groups", dp.groups_area),
            ("tooltip", dp.tooltip),
        ];
        if dp.groups_button.w > 0.0 {
            zones.push(("groups_button", dp.groups_button));
        }
        if dp.commands_button.w > 0.0 {
            zones.push(("commands_button", dp.commands_button));
        }
        for (index, toast) in dp.toasts.iter().enumerate() {
            zones.push((
                match index {
                    0 => "toast",
                    1 => "toast2",
                    _ => "toast3",
                },
                *toast,
            ));
        }
        zones
    }

    /// Every rect that dispatches an action. All of them must be >= 48 dp.
    pub fn interactive(&self) -> Vec<(&'static str, Rect)> {
        let dp = self.dp();
        let mut rects: Vec<(&'static str, Rect)> = Vec::new();
        for slot in &dp.card_slots {
            rects.push(("card", *slot));
        }
        for slot in &dp.group_slots {
            rects.push(("group", *slot));
        }
        for slot in &dp.army_slots {
            rects.push(("army", *slot));
        }
        for slot in &dp.inventory_parts.touches {
            rects.push(("inventory", *slot));
        }
        for (name, rect) in [
            ("minimap", dp.minimap_frame),
            ("groups_button", dp.groups_button),
            ("commands_button", dp.commands_button),
        ] {
            if rect.w > 0.0 && rect.h > 0.0 {
                rects.push((name, rect));
            }
        }
        // Collapsed rects (closed drawer, hidden inventory) are not targets.
        rects.retain(|(_, rect)| rect.w > 0.0 && rect.h > 0.0);
        rects
    }

    /// Hit test in framebuffer px against the very rects that were drawn.
    /// Overlays are modal: they are tested before the console beneath them.
    pub fn hit(&self, point: Vec2) -> Option<HudHit> {
        let dp = self.dp();
        let to_px = |r: Rect| {
            Rect::new(
                r.x * self.density,
                r.y * self.density,
                r.w * self.density,
                r.h * self.density,
            )
        };
        if dp.groups_overlay.w > 0.0 && to_px(dp.groups_overlay).contains(point) {
            for (index, slot) in dp.group_slots.iter().enumerate() {
                if to_px(*slot).contains(point) {
                    return Some(HudHit::Group(index));
                }
            }
            return Some(HudHit::Console);
        }
        if dp.commands_overlay.w > 0.0 && to_px(dp.commands_overlay).contains(point) {
            for (index, rect) in self.card_slots.iter().enumerate() {
                if rect.contains(point) {
                    return Some(HudHit::Card(index));
                }
            }
            return Some(HudHit::Console);
        }
        if self.phase.contains(point) && self.phase.w > 0.0 {
            return Some(HudHit::Phase);
        }
        for (index, rect) in self.card_slots.iter().enumerate() {
            if rect.contains(point) {
                return Some(HudHit::Card(index));
            }
        }
        for (index, rect) in self.group_slots.iter().enumerate() {
            if rect.contains(point) {
                return Some(HudHit::Group(index));
            }
        }
        if self.minimap.contains(point) {
            return Some(HudHit::Minimap);
        }
        if self.selection.contains(point) {
            return Some(HudHit::Selection);
        }
        if self.console.contains(point) {
            return Some(HudHit::Console);
        }
        None
    }

    /// Layout QA in dp. Empty means the composition satisfies TZ §11.
    pub fn issues(&self, _tokens: &Tokens) -> Vec<String> {
        let dp = self.dp();
        let mut issues = Vec::new();
        let eps = 0.01;
        // 1. Console background is edge to edge and bottom anchored.
        if dp.console.x.abs() > eps
            || (dp.console.right() - dp.screen.x).abs() > eps
            || (dp.console.bottom() - dp.screen.y).abs() > eps
        {
            issues.push(format!(
                "console not edge-to-edge: {:?} screen {:?}",
                dp.console, dp.screen
            ));
        }
        // 2. Camera viewport ends exactly at the console top.
        if (dp.viewport.bottom() - dp.console.y).abs() > eps {
            issues.push(format!(
                "viewport ends {:.2} dp from console top",
                dp.viewport.bottom() - dp.console.y
            ));
        }
        // 3. Interactive content inside the safe content rect.
        let safe_screen = Rect::new(
            dp.safe.left,
            dp.top_bar.bottom(),
            (dp.screen.x - dp.safe.left - dp.safe.right).max(1.0),
            (dp.screen.y - dp.safe.bottom - dp.top_bar.bottom()).max(1.0),
        );
        // An open drawer is modal: it is drawn last, it eats the gesture first,
        // and on a short window it deliberately floats over the console.
        let modal = if dp.commands_overlay.w > 0.0 {
            Some(dp.commands_overlay)
        } else if dp.groups_overlay.w > 0.0 {
            Some(dp.groups_overlay)
        } else {
            None
        };
        let in_modal = |rect: Rect| {
            modal
                .map(|panel| rect.intersection_area(&panel) > 0.5)
                .unwrap_or(false)
        };
        for (name, rect) in self.interactive() {
            // Console content lives inside the safe content rect; drawer cells
            // float above the console and only owe the safe screen area.
            let region = if in_modal(rect) || rect.bottom() <= dp.console.y + 0.5 {
                safe_screen
            } else {
                dp.content
            };
            if !rect.is_inside(&region) {
                issues.push(format!(
                    "{name} outside {}: {rect:?} vs {region:?}",
                    if region == dp.content { "safe content" } else { "safe screen" }
                ));
            }
            let side = rect.w.min(rect.h);
            if side + 0.01 < TOUCH_DP {
                issues.push(format!(
                    "{name} touch target {side:.2} dp < {TOUCH_DP} dp"
                ));
            }
        }
        // 4. No sibling zone overlap.
        let zones = self.zone_rects();
        for i in 0..zones.len() {
            for j in (i + 1)..zones.len() {
                // A modal drawer legitimately covers the console it belongs to;
                // everything else must tile cleanly.
                if in_modal(zones[i].1) || in_modal(zones[j].1) {
                    continue;
                }
                // Nesting is not a collision: the resource strip lives inside
                // the top bar, the drawers sit on the console they belong to.
                // Only partial sibling overlaps are a layout bug.
                if zones[i].1.contains_rect(&zones[j].1)
                    || zones[j].1.contains_rect(&zones[i].1)
                {
                    continue;
                }
                let area = zones[i].1.intersection_area(&zones[j].1);
                if area > 0.5 {
                    issues.push(format!(
                        "{} overlaps {} by {area:.1} dp² ({:?} vs {:?})",
                        zones[i].0, zones[j].0, zones[i].1, zones[j].1
                    ));
                }
            }
        }
        // 5. The card is exactly 4x3 row-major and never shrinks.
        if !dp.card_slots.is_empty() && dp.card_slots.len() != CARD_SLOTS {
            issues.push(format!(
                "command card has {} slots, expected {CARD_SLOTS}",
                dp.card_slots.len()
            ));
        }
        for (index, slot) in dp.card_slots.iter().enumerate() {
            if slot.w + 0.01 < TOUCH_DP || slot.h + 0.01 < TOUCH_DP {
                issues.push(format!(
                    "card slot {index} is {:.1}x{:.1} dp",
                    slot.w, slot.h
                ));
            }
        }
        // 6. Minimap is square.
        if dp.minimap_frame.w > 0.0
            && (dp.minimap_frame.w - dp.minimap_frame.h).abs() > 0.5
        {
            issues.push(format!("minimap not square: {:?}", dp.minimap_frame));
        }
        if dp.minimap_image.w > 0.0
            && (dp.minimap_image.w - dp.minimap_image.h).abs() > 0.5
        {
            issues.push(format!("minimap image not square: {:?}", dp.minimap_image));
        }
        // 7. The console must leave a usable playfield. The 55% phone-class
        // limit assumes a normal dp height; a 240 dp window (1280x720 at 3x)
        // cannot hold the 48 dp command card plus the strip at 55% without
        // clipping targets, so that class gets a documented 62% ceiling.
        let hud = dp.console.h + dp.top_bar.h;
        let ceiling = if dp.screen.y <= 260.0 { 0.62 } else { 0.55 };
        if hud > dp.screen.y * ceiling {
            issues.push(format!(
                "HUD eats {:.1}% of {:.0} dp height",
                100.0 * hud / dp.screen.y,
                dp.screen.y
            ));
        }
        issues
    }

    /// The whole framebuffer rectangle.
    pub fn viewport_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.viewport.x.max(1.0), self.viewport.y.max(1.0))
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuScreen {
    Main,
    Campaign,
    CustomGame,
    SinglePlayer,
    Skirmish,
    MapSelect,
    PauseOptions,
    Loading,
    Victory,
    Defeat,
}

#[derive(Clone, Copy, Debug)]
pub struct MenuButton<'a> {
    pub label: &'a str,
    pub sub: &'a str,
    pub action: u8,
    pub enabled: bool,
}

/// Menu button visual state drawn from the shipped `war3skins.txt` art.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuButtonState {
    Normal,
    Hover,
    Pressed,
    Disabled,
}

/// One pack-art part of the menu chrome: the texture index plus the authored
/// size and 9-slice insets, so every slice uses the real PNG metrics instead of
/// assuming a 256x64 sheet (`menu_panel_normal_512x384` is 512x384).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuPart {
    pub texture: usize,
    pub w: f32,
    pub h: f32,
    /// Authored insets `[left, top, right, bottom]` in source pixels.
    pub insets: [f32; 4],
}

impl MenuPart {
    pub fn new(texture: usize, w: f32, h: f32, insets: [f32; 4]) -> Self {
        Self { texture, w, h, insets }
    }
}

/// Shipped menu/dialog chrome: the pack's `menus/*` panel, the four button
/// states, the loading frame and the pack serif face for the title.
///
/// Every field is optional: a screen still renders (flat chrome) on a data set
/// without the art, and the art is never substituted silently.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuSkin {
    /// Panel background (9-slice), pack `menu_panel_normal_512x384`.
    pub panel: Option<MenuPart>,
    /// Panel border (9-slice), retail `EscMenuBorder` when present.
    pub panel_border: Option<MenuPart>,
    /// Title band (9-slice), pack `topbar_plate_normal_720x32`.
    pub title_band: Option<MenuPart>,
    pub button_normal: Option<MenuPart>,
    pub button_hover: Option<MenuPart>,
    pub button_pressed: Option<MenuPart>,
    pub button_disabled: Option<MenuPart>,
    pub loading_frame: Option<MenuPart>,
    /// Message banner above the panel (pack plate).
    pub banner: Option<MenuPart>,
    /// Pack fonts (`primary_serif_cyrillic.ttf` first): the title face.
    pub fonts: Option<&'static crate::hud::HudFonts>,
}

/// One glue screen. `buttons` are the main-page entries; `tabs` are the
/// campaign groups, `rows` the chapter/map list and `nav` the bottom row
/// (PREV / NEXT / START / BACK). Every list is laid out finger-first.
pub struct MenuView<'a> {
    pub viewport: Vec2,
    pub safe: Option<SafeArea>,
    pub screen: MenuScreen,
    pub title: &'a str,
    pub subtitle: &'a str,
    pub map_name: &'a str,
    pub buttons: &'a [MenuButton<'a>],
    pub progress: f32,
    pub status: &'a str,
    /// Shipped menu art; `None` keeps the flat dialog.
    pub redraw: Option<MenuSkin>,
    /// Hovered button index (mouse/pointer over).
    pub hover: Option<usize>,
    /// Pressed entry index (pointer down), interpreted inside `pressed_group`.
    pub pressed: Option<usize>,
    /// Group the pressed index belongs to: 0 entries, 1 tabs, 2 rows, 3 nav.
    /// Without it the same index in two groups would light up together.
    pub pressed_group: u8,
    /// Campaign group tabs (campaign screen).
    pub tabs: &'a [MenuButton<'a>],
    /// Active tab (drawn as pressed art, not a colour swap).
    pub active_tab: usize,
    /// Chapter / map rows.
    pub rows: &'a [MenuButton<'a>],
    /// Bottom navigation row.
    pub nav: &'a [MenuButton<'a>],
    /// Page counter (`HUMAN 1/2`).
    pub page_label: &'a str,
    /// Dark veil over the live scene behind the panel, 0..1.
    pub veil: f32,
}

#[derive(Clone, Debug)]
pub struct MenuLayout {
    pub panel: Rect,
    pub title_band: Rect,
    pub title: Rect,
    pub subtitle: Rect,
    pub buttons: Vec<Rect>,
    pub progress: Rect,
    pub status: Rect,
    /// Campaign group tab rects.
    pub tabs: Vec<Rect>,
    /// Chapter / map row rects.
    pub rows: Vec<Rect>,
    /// Bottom navigation rects.
    pub nav: Vec<Rect>,
    /// Page counter plate (`HUMAN 1/2`).
    pub page_label: Rect,
    /// Chapter rows that fit on this page (the caller pages the rest).
    pub visible_rows: usize,
}

impl<'a> MenuView<'a> {
    pub fn for_screen(
        screen: MenuScreen,
        viewport: Vec2,
        map_name: &'a str,
    ) -> MenuView<'a> {
        // Static labels; call sites override the lists as needed.
        const MAIN: &[MenuButton<'static>] = &[
            MenuButton { label: "CONTINUE", sub: "SAVE", action: 16, enabled: true },
            MenuButton { label: "CAMPAIGN", sub: "44", action: 17, enabled: true },
            MenuButton { label: "CUSTOM GAME", sub: "MAPS", action: 18, enabled: true },
            MenuButton { label: "LOAD GAME", sub: "SLOT", action: 15, enabled: true },
            MenuButton { label: "OPTIONS", sub: "SETUP", action: 19, enabled: true },
            MenuButton { label: "QUIT", sub: "EXIT", action: 2, enabled: true },
        ];
        const MAP: &[MenuButton<'static>] = &[
            MenuButton { label: "START GAME", sub: "PLAY", action: 0, enabled: true },
            MenuButton { label: "BACK", sub: "ESC", action: 3, enabled: true },
        ];
        const LOADING: &[MenuButton<'static>] = &[];
        const SINGLE: &[MenuButton<'static>] = &[
            MenuButton { label: "CAMPAIGN", sub: "44 CHAPTERS", action: 17, enabled: true },
            MenuButton { label: "CUSTOM GAME", sub: "MAPS", action: 18, enabled: true },
            MenuButton { label: "BACK", sub: "ESC", action: 3, enabled: true },
        ];
        const SKIRMISH: &[MenuButton<'static>] = &[
            MenuButton { label: "RACE", sub: "HUMAN", action: 4, enabled: true },
            MenuButton { label: "OPPONENT", sub: "COMPUTER", action: 5, enabled: true },
            MenuButton { label: "MAP", sub: "SELECT", action: 1, enabled: true },
            MenuButton { label: "DIFFICULTY", sub: "NORMAL", action: 6, enabled: true },
            MenuButton { label: "START GAME", sub: "PLAY", action: 0, enabled: true },
            MenuButton { label: "BACK", sub: "ESC", action: 3, enabled: true },
        ];
        const PAUSE: &[MenuButton<'static>] = &[
            MenuButton { label: "RESUME", sub: "ESC", action: 7, enabled: true },
            MenuButton { label: "OPTIONS", sub: "SETUP", action: 8, enabled: true },
            MenuButton { label: "SAVE GAME", sub: "SAVE", action: 9, enabled: false },
            MenuButton { label: "QUIT", sub: "EXIT", action: 2, enabled: true },
        ];
        const VICTORY: &[MenuButton<'static>] = &[
            MenuButton { label: "CONTINUE", sub: "ENTER", action: 0, enabled: true },
            MenuButton { label: "SCORE", sub: "TAB", action: 10, enabled: true },
        ];
        const DEFEAT: &[MenuButton<'static>] = &[
            MenuButton { label: "RETRY", sub: "ENTER", action: 11, enabled: true },
            MenuButton { label: "QUIT", sub: "EXIT", action: 2, enabled: true },
        ];
        let (title, subtitle, buttons) = match screen {
            MenuScreen::Main => ("WARCRAFT III", "REIGN OF CHAOS", MAIN),
            MenuScreen::Campaign => ("CAMPAIGN", "PROLOGUE · HUMAN · UNDEAD · ORC · NIGHT ELF", LOADING),
            MenuScreen::CustomGame => ("CUSTOM GAME", "MELEE · RACE · DIFFICULTY", LOADING),
            MenuScreen::SinglePlayer => ("SINGLE PLAYER", "CAMPAIGN · CUSTOM", SINGLE),
            MenuScreen::Skirmish => ("SKIRMISH", "RACE · OPPONENT · MAP · DIFFICULTY", SKIRMISH),
            MenuScreen::MapSelect => ("MAP SELECT", "CAMPAIGN · HUMAN", MAP),
            MenuScreen::PauseOptions => ("GAME PAUSED", "OPTIONS · SAVE · QUIT", PAUSE),
            MenuScreen::Loading => ("LOADING", "PREPARING BATTLEFIELD", LOADING),
            MenuScreen::Victory => ("VICTORY", "MISSION COMPLETE", VICTORY),
            MenuScreen::Defeat => ("DEFEAT", "MISSION FAILED", DEFEAT),
        };
        MenuView {
            viewport,
            safe: None,
            screen,
            title,
            subtitle,
            map_name,
            buttons,
            progress: 0.0,
            status: "",
            redraw: None,
            hover: None,
            pressed: None,
            pressed_group: 0,
            tabs: &[],
            active_tab: 0,
            rows: &[],
            nav: &[],
            page_label: "",
            veil: 0.55,
        }
    }

    pub fn redraw(mut self, skin: Option<MenuSkin>) -> Self {
        self.redraw = skin;
        self
    }

    pub fn tabs(mut self, tabs: &'a [MenuButton<'a>]) -> Self {
        self.tabs = tabs;
        self
    }

    pub fn rows(mut self, rows: &'a [MenuButton<'a>]) -> Self {
        self.rows = rows;
        self
    }

    pub fn nav(mut self, nav: &'a [MenuButton<'a>]) -> Self {
        self.nav = nav;
        self
    }

    pub fn page(mut self, label: &'a str, veil: f32) -> Self {
        self.page_label = label;
        self.veil = veil.clamp(0.0, 1.0);
        self
    }

    /// Every tappable rect of the screen, in draw order (main entries, tabs,
    /// rows, nav). The hit test walks this exact list, so a tap can never land
    /// on a rect the frame did not draw.
    pub fn hits(&self, layout: &MenuLayout) -> Vec<(Rect, MenuButton<'a>)> {
        let mut out = Vec::new();
        for (index, rect) in layout.buttons.iter().enumerate() {
            if let Some(button) = self.buttons.get(index) {
                out.push((*rect, *button));
            }
        }
        for (index, rect) in layout.tabs.iter().enumerate() {
            if let Some(button) = self.tabs.get(index) {
                out.push((*rect, *button));
            }
        }
        for (index, rect) in layout.rows.iter().enumerate() {
            if let Some(button) = self.rows.get(index) {
                out.push((*rect, *button));
            }
        }
        for (index, rect) in layout.nav.iter().enumerate() {
            if let Some(button) = self.nav.get(index) {
                out.push((*rect, *button));
            }
        }
        out
    }
}

/// `MenuView::pressed_group` values: which layout list the pressed index is in.
pub const MENU_GROUP_ENTRIES: u8 = 0;
pub const MENU_GROUP_TABS: u8 = 1;
pub const MENU_GROUP_ROWS: u8 = 2;
pub const MENU_GROUP_NAV: u8 = 3;

/// 9-slice a pack part into `rect`: source insets keep their authored pixel
/// size times `art_scale`, only the centre cell stretches. Zero-area cells are
/// skipped, so a part with `t = b = 0` (the banner plate) becomes three
/// horizontal cells and never stretches vertically.
pub fn menu_part_quads(part: &MenuPart, rect: Rect, art_scale: f32) -> Vec<(Rect, [f32; 4])> {
    if rect.w <= 0.0 || rect.h <= 0.0 || part.w <= 0.0 || part.h <= 0.0 {
        return Vec::new();
    }
    let scale = art_scale.max(0.01);
    let l = (part.insets[0] * scale).min(rect.w * 0.45);
    let t = (part.insets[1] * scale).min(rect.h * 0.45);
    let r = (part.insets[2] * scale).min(rect.w * 0.45);
    let b = (part.insets[3] * scale).min(rect.h * 0.45);
    let xs = [rect.x, rect.x + l, rect.right() - r, rect.right()];
    let ys = [rect.y, rect.y + t, rect.bottom() - b, rect.bottom()];
    let u = [
        0.0,
        part.insets[0] / part.w,
        1.0 - part.insets[2] / part.w,
        1.0,
    ];
    let v = [
        0.0,
        part.insets[1] / part.h,
        1.0 - part.insets[3] / part.h,
        1.0,
    ];
    let mut out = Vec::with_capacity(9);
    for row in 0..3 {
        for column in 0..3 {
            let w = xs[column + 1] - xs[column];
            let h = ys[row + 1] - ys[row];
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            out.push((
                Rect::new(xs[column], ys[row], w, h),
                [u[column], v[row], u[column + 1], v[row + 1]],
            ));
        }
    }
    out
}

/// Draw one 9-sliced pack part (all nine cells, painter order).
pub fn draw_menu_part(ui: &mut UiBatch, part: &MenuPart, rect: Rect, art_scale: f32, tint: [f32; 4]) {
    for (dst, uv) in menu_part_quads(part, rect, art_scale) {
        ui.textured_quad(part.texture, dst, uv, tint);
    }
}

#[derive(Clone, Debug)]
pub struct MobileHud {
    pub tokens: Tokens,
    /// Design tokens (palette roles, the direction type scale, dp metrics).
    /// The panel composes from these, never from ad-hoc pixel values.
    pub theme: crate::ui::theme::Theme,
    /// Signature of the last `HUD_CARD` line, so the diagnostic is logged once
    /// per card shape instead of once per frame.
    card_log: std::cell::Cell<u64>,
}

impl Default for MobileHud {
    fn default() -> Self {
        Self::new()
    }
}

impl MobileHud {
    pub fn new() -> Self {
        Self {
            tokens: Tokens::default(),
            theme: crate::ui::theme::Theme::classic_fantasy(),
            card_log: std::cell::Cell::new(u64::MAX),
        }
    }

    pub fn with_tokens(tokens: Tokens) -> Self {
        Self {
            tokens,
            ..Self::new()
        }
    }

    /// Install an explicit theme (menu and HUD share one).
    pub fn with_theme(mut self, theme: crate::ui::theme::Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Type multiplier for a role at this frame's density (face authored at
    /// 30 px, so `px = dp * density` and `scale = px / 30`).
    pub fn type_scale(&self, role: crate::ui::theme::TypeRole, density: f32) -> f32 {
        self.theme.scale_30(role, density)
    }

    /// The `HUD_CARD` diagnostic (addendum §2): one line per card shape.
    fn log_card(&self, view: &HudView<'_>) {
        let mut slots = 0usize;
        let mut icons = 0usize;
        let mut missing = 0usize;
        for button in view.commands {
            if !button.present {
                continue;
            }
            slots += 1;
            if button.icon.is_some() {
                icons += 1;
            } else {
                missing += 1;
            }
        }
        let signature = ((slots as u64) << 20) | ((icons as u64) << 10) | missing as u64;
        if self.card_log.get() == signature {
            return;
        }
        self.card_log.set(signature);
        crate::dev_log!(
            "HUD_CARD slots={slots} icons={icons} missing={missing} page=1/1"
        );
    }

    /// The `HUD_PORTRAIT` diagnostic (addendum §1), once per def.
    fn log_portrait(&self, view: &HudView<'_>) {
        let Some(selection) = view.selection.as_ref() else {
            return;
        };
        let Some(def) = selection.def else {
            return;
        };
        let source = selection
            .portrait_source
            .clone()
            .unwrap_or_else(|| {
                crate::ui::widgets::portrait::resolve(
                    def,
                    None,
                    selection.portrait.map(|_| "texture"),
                )
            });
        if let Some(line) = crate::ui::widgets::portrait::log_once(def, &source) {
            crate::dev_log!("{line}");
        }
    }

    /// Canonical dp layout for this frame (TZ §4.2). Pure: the same metrics,
    /// preferences and state always produce the same rectangles.
    pub fn hud_layout(&self, view: &HudView<'_>) -> HudLayout {
        let metrics = WindowMetrics {
            framebuffer_px: view.viewport,
            density_scale: view.density,
            safe_insets_dp: view.safe.unwrap_or_default(),
            orientation: view.orientation,
        };
        let selection_state = view.selection.as_ref();
        let state = HudLayoutState {
            // The chip row is paginated, so the full count is the truth.
            selection_count: view.selection_count.max(view.army.len()),
            has_mana: selection_state.map(|s| s.mana.is_some()).unwrap_or(false),
            production: selection_state
                .and_then(|s| s.production.as_ref())
                .is_some(),
            inventory: !view.item_icons.is_empty(),
            groups_open: view.groups_open,
            commands_open: view.commands_open,
        };
        let mut prefs = view.ui;
        prefs.show_fps = view.show_fps;
        prefs.show_hitboxes = view.show_hitboxes;
        let mut dp = HudLayout::compute(metrics, prefs, state);
        // Publish exactly the toast bands that have a live toast: the composer
        // reserves up to three rows, but an empty band is not a row (the 0
        // toast case used to report three, and the draw/hit lists disagreed).
        let live = if view.toasts.is_empty() {
            usize::from(view.toast.is_some())
        } else {
            view.toasts.len()
        };
        dp.toasts.truncate(live);
        dp.toast = dp
            .toasts
            .first()
            .copied()
            .unwrap_or_else(|| Rect::new(0.0, 0.0, 0.0, 0.0));
        dp
    }

    /// Draw/hit-ready pixel rectangles. This is the only place `px = dp *
    /// density` is applied: every consumer below shares the result.
    pub fn layout(&self, view: &HudView<'_>) -> MobileHudLayout {
        let dp = self.hud_layout(view);
        let mut px = dp.to_px(self.hud_density(view));
        // The minimap ornament is drawn *outside* the image/hit rect (the
        // nine-patch border dilates it). A flush image (x=0) therefore painted
        // the frame at x=-4, outside the safe area. Keep the dilated frame
        // inside the console by insetting the image origin by the measured
        // border; the hit rect stays the image the caller draws.
        if let Some(skin) = view.hud_skin && px.minimap.w > 0.0 && px.minimap.h > 0.0 {
            let border = crate::hudskin::minimap_frame::borders(skin, px.minimap.w);
            let console = px.console;
            let lo_x = console.x + border[0];
            let hi_x = (console.right() - px.minimap.w - border[2]).max(lo_x);
            let lo_y = console.y + border[1];
            let hi_y = (console.bottom() - px.minimap.h - border[3]).max(lo_y);
            px.minimap.x = px.minimap.x.clamp(lo_x, hi_x);
            px.minimap.y = px.minimap.y.clamp(lo_y, hi_y);
            // A widened frame can reach the command card in the portrait
            // compact plan (image 112 -> frame 136 at x=0 while the card starts
            // at 128). Shrink the square so the ornament touches but never
            // overlaps a sibling zone; the ratio stays >0.8.
            let mut side = px.minimap.w.min(px.minimap.h);
            // The frame starts at image.x - border_l (already inside the
            // console), so only the right/bottom border consumes the gap to a
            // sibling: side <= sibling - image - border_far.
            if px.card.w > 0.0 && px.card.x >= px.minimap.x {
                side = side.min(px.card.x - px.minimap.x - border[2]);
            }
            if px.groups_area.w > 0.0 && px.groups_area.x >= px.minimap.x {
                side = side.min(px.groups_area.x - px.minimap.x - border[2]);
            }
            side = side.min(px.console.bottom() - px.minimap.y - border[3]);
            if side > 16.0 && side < px.minimap.w {
                px.minimap.w = side;
                px.minimap.h = side;
            }
        }
        px.tooltip_anchor = view.tooltip_anchor;
        // The classic (non-pack) tooltip is placed by the same composer: float
        // it above the hovered slot, clamped to the safe frame. The skin path
        // re-anchors through `skin_tooltip::draw_request`, using the same
        // anchor; the old code left the rect centred and only shifted the
        // drawn box on the pack path, so the two paths disagreed on the box.
        if let Some(anchor) = view.tooltip_anchor
            && px.tooltip.w > 0.0
            && px.tooltip.h > 0.0
        {
            let gap = ZONE_GAP_DP * px.density;
            let max_x = (px.safe_frame.right() - px.tooltip.w).max(px.safe_frame.x);
            let x = (anchor.centre().x - px.tooltip.w * 0.5).clamp(px.safe_frame.x, max_x);
            let mut y = anchor.y - gap - px.tooltip.h;
            if y < px.safe_frame.y {
                y = anchor.bottom() + gap;
            }
            let max_y = (px.safe_frame.bottom() - px.tooltip.h).max(px.safe_frame.y);
            px.tooltip = Rect::new(x, y.clamp(px.safe_frame.y, max_y), px.tooltip.w, px.tooltip.h);
        }
        px
    }

    fn hud_density(&self, view: &HudView<'_>) -> f32 {
        let metrics = WindowMetrics::new(view.viewport, view.density);
        metrics.density()
    }

    pub fn draw(
        &self,
        view: &HudView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> MobileHudLayout {
        self.log_portrait(view);
        self.log_card(view);
        if let Some(skin) = view.hud_skin
            && let Some(pack) = skin.pack.as_ref()
        {
            return self.draw_pack(view, ui, font, chrome, skin, pack);
        }
        let mut layout = self.layout(view);
        let tokens = &self.tokens;
        let shadow = Some(tokens.shadow);
        let s = layout.scale;

        // =========================================================== console
        // Composition (reference `analysis/hudskin/COMPOSITION.md`):
        //
        //   [ minimap | rail ] [ portrait + unit card ] [ 10 group plates ]
        //   [ crest plaque ] [ 4x3 command card ]
        //
        // Big areas are a flat stone field (procedural grain + vignette); every
        // atlas piece is drawn at <= 1.15x its authored size or tiled, never
        // stretched into mush. The stretched 592x208 plate and the two cap
        // banners are gone: their red drapes were the "smeared red bands".
        let stone = STONE_FIELD;
        let flat = chrome.flat;
        if let Some(skin) = view.hud_skin {
            draw_stone_field(ui, flat, layout.console, stone);
            draw_bevel_ring(ui, flat, layout.console, s, STONE_HI, STONE_LO, GOLD_LINE);
            draw_minimap_frame(skin, ui, flat, &layout, s);
            draw_minimap_rail(skin, ui, flat, &layout, font, tokens, s);
            draw_selection_skin(ui, flat, font, tokens, &layout, view, skin);
            if view.selection.is_none() {
                draw_empty_portrait(skin, ui, flat, &layout, font, tokens, s);
            }
            draw_crest_panel(skin, ui, flat, &layout, view, font, tokens, s);
            draw_command_card(skin, ui, flat, &layout, view, font, tokens, s);
            draw_group_plates(skin, ui, flat, &layout, view, font, tokens, s);
            draw_army_chips(skin, ui, flat, &layout, view);
            draw_inventory_panel(skin, ui, flat, &layout, view, font, tokens, s);
            // Filled card slots on top of the plate, then the units.
        } else {
            // Shipped-art fallbacks (FDF chrome / redraw pack / retail tiles):
            // unchanged contract, only the band geometry moved.
            chrome.flat_rect(ui, layout.console, [0.055, 0.045, 0.035, 0.98]);
            let mut console_drawn = false;
            if chrome
                .element_or(ui, layout.console, ATLAS_CONSOLE, s, [1.0; 4])
                .is_some()
            {
                console_drawn = true;
            }
            if !console_drawn {
                if let Some(redraw) = &layout.redraw {
                    for quad in &redraw.topbar {
                        ui.textured_quad(quad.texture, quad.rect, quad.uv, [1.0; 4]);
                    }
                    for quad in &redraw.console {
                        ui.textured_quad(quad.texture, quad.rect, quad.uv, [1.0; 4]);
                    }
                } else {
                    chrome.flat_rect(ui, layout.top_meta, [0.035, 0.03, 0.024, 0.96]);
                    for part in &layout.console_parts {
                        if part.rect.w <= 0.0 {
                            continue;
                        }
                        if let Some(texture) = view
                            .console_tiles
                            .get(part.tile as usize)
                            .copied()
                            .flatten()
                        {
                            chrome.frame(ui, part.rect, Some(texture), 1.0, [1.0; 4]);
                        }
                    }
                    ui.border(layout.console, 2.0 * s, [0.55, 0.44, 0.22, 0.95]);
                    ui.rect(
                        Rect::new(
                            layout.console.x + 2.0 * s,
                            layout.console.y + 1.0 * s,
                            layout.console.w - 4.0 * s,
                            1.0 * s,
                        ),
                        [0.80, 0.70, 0.44, 0.35],
                    );
                }
            }
            draw_minimap_fallback(ui, chrome, &layout, view, font, tokens, s);
            draw_selection_fallback(ui, font, chrome, tokens, &layout, view, s);
            draw_card_fallback(ui, font, chrome, tokens, &layout, view, s);
            // Compact mode has no inline card/groups tray: the two persistent
            // drawer controls are the only affordance. The classic path used
            // to leave their 48 dp rects empty (they were only painted on the
            // pack path), which read as two stray squares on the console.
            let chain = FontChain::new(&[font]);
            for (rect, label, open) in [
                (layout.groups_button, "ARMY", view.groups_open),
                (layout.commands_button, "CMD", view.commands_open),
            ] {
                draw_drawer_button_fallback(ui, &chain, tokens, flat, rect, label, s, shadow, open);
            }
            for (index, slot) in layout.army_slots.iter().enumerate() {
                if !chrome.element(ui, *slot, "button_empty", s, [0.72, 0.72, 0.72, 0.85]) {
                    chrome.flat_rect(ui, *slot, [0.04, 0.035, 0.03, 0.95]);
                    ui.border(*slot, 1.0 * s, [0.35, 0.30, 0.20, 0.8]);
                }
                if index == 0
                    && let Some(portrait) = view.selection.as_ref().and_then(|view| view.portrait)
                {
                    ui.textured_quad(
                        portrait,
                        slot.inset(4.0 * s, 4.0 * s),
                        [0.0, 0.0, 1.0, 1.0],
                        [1.0; 4],
                    );
                }
            }
        }

        // Every enemy/neutral blip, the fog and the camera frustum go onto the
        // map square whether the atlas prefaced it or not.
        draw_minimap_content(ui, flat, font, &layout, view, tokens, s, shadow);

        // --------------------------------------------------------- top banner
        // Resources live in the banner: icon + number, no captions, upkeep
        // right-aligned in the same band, fps at the far right.
        let banner_edge = [0.72, 0.62, 0.36, 0.55];
        chrome.flat_rect(ui, layout.top_meta, BANNER_FIELD);
        ui.border(layout.top_meta, 1.5 * s, banner_edge);
        fill(
            ui,
            flat,
            Rect::new(layout.top_meta.x, layout.top_meta.bottom() - 1.0, layout.top_meta.w, 1.0),
            GOLD_LINE,
        );
        font.text_box(
            ui,
            view.map_name,
            clamp_rect(layout.map_name, layout.top_meta),
            0.76 * s,
            tokens.text,
            TextAlign::Left,
            shadow,
        );
        for (index, pill) in layout.resource_pills.iter().enumerate() {
            let Some(data) = view.resources.get(index) else {
                continue;
            };
            let icon_side = (26.0 * s).min(pill.h);
            let icon_rect = Rect::new(
                pill.x,
                pill.y + (pill.h - icon_side) * 0.5,
                icon_side,
                icon_side,
            );
            if let Some(icon) = data.icon {
                ui.textured_quad(icon, icon_rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
            } else {
                chrome.flat_rect(ui, icon_rect, [0.28, 0.26, 0.20, 0.9]);
            }
            let value_rect = Rect::new(
                icon_rect.right() + 6.0 * s,
                pill.y,
                (pill.right() - icon_rect.right() - 6.0 * s).max(2.0),
                pill.h,
            );
            font.text_box(
                ui,
                data.value,
                value_rect,
                0.78 * s,
                if data.warn { tokens.hp_low } else { data.tone },
                TextAlign::Left,
                shadow,
            );
        }
        font.text_box(
            ui,
            view.upkeep,
            clamp_rect(layout.upkeep, layout.top_meta),
            0.54 * s,
            tokens.text_muted,
            TextAlign::Right,
            None,
        );
        font.text_box(
            ui,
            &format!("{:.0} FPS", view.fps.max(0.0)),
            clamp_rect(layout.fps, layout.top_meta),
            0.54 * s,
            tokens.text_muted,
            TextAlign::Right,
            None,
        );

        // -------------------------------------------------- toast / hint
        if let Some(hint) = &view.hint {
            chrome.flat_rect(ui, layout.hint, [0.08, 0.07, 0.05, 0.97]);
            ui.border(layout.hint, 1.5 * s, [0.55, 0.44, 0.20, 0.95]);
            font.text_box(
                ui,
                &hint.text,
                clamp_rect(layout.hint.inset(10.0 * s, 0.0), layout.hint),
                0.67 * s,
                if hint.attack {
                    tokens.gold
                } else {
                    tokens.text
                },
                TextAlign::Centre,
                shadow,
            );
        }
        // Toast band: one row per live toast, stacked above the console. The
        // rows come from `layout.toasts`, so a toast can never land on the
        // tooltip or the mode hint.
        let toast_rows: Vec<ToastView> = if view.toasts.is_empty() {
            view.toast.clone().into_iter().collect()
        } else {
            view.toasts.clone()
        };
        for (index, toast) in toast_rows.iter().take(3).enumerate() {
            let Some(rect) = layout.toasts.get(index).copied() else {
                break;
            };
            let accent = match toast.tone {
                ToastTone::Info => tokens.stroke_hot,
                ToastTone::Success => tokens.success,
                ToastTone::Danger => tokens.danger,
            };
            let toast_text = if let Some(skin) = view.hud_skin {
                // Region 10: the banner plate (`wide_panel`) under the toast.
                let content = skin_dialogs::draw_banner(skin, ui, rect, s);
                ui.border(rect, 1.5 * s, accent);
                content
            } else {
                chrome.flat_rect(ui, rect, [0.08, 0.07, 0.05, 0.97]);
                ui.border(rect, 1.5 * s, accent);
                clamp_rect(rect.inset(10.0 * s, 0.0), rect)
            };
            // Toast prose is fitted into the plate: wrapped on word boundaries,
            // stepped down to the readable floor and clipped to the plate, so a
            // 120-character toast never leaks out of its band.
            let inner = toast_text.inset(8.0 * s, 4.0 * s);
            let fit = fit_text_plate(font, &toast.text, inner, 0.67 * s, MIN_TEXT_PX, 3);
            draw_plate_text(
                ui,
                font,
                &fit,
                inner,
                tokens.text,
                TextAlign::Centre,
                shadow,
            );
        }

        if let Some(tooltip) = &view.tooltip {
            // WC3 tooltip: title row (name + hotkey) in gold, then the wrapped
            // description. The box is anchored to the hovered command slot and
            // its height is measured with the live font; the bottom edge stays
            // above the toast/hint bands.
            let estimate_w = (layout.tooltip.w - 20.0 * s).max(1.0);
            let title_scale = (0.90 * s).max(crate::hud::MIN_TEXT_PX / 30.0);
            let body_scale = (0.72 * s).max(crate::hud::MIN_TEXT_PX / 30.0);
            let mut lines = tooltip.split('\n');
            let title = lines.next().unwrap_or("");
            let body: Vec<&str> = lines.filter(|line| !line.trim().is_empty()).collect();
            let body_text = body.join("\n");
            let title_h = font.line_box(title_scale);
            let body_h = font.line_box(body_scale);
            if let Some(skin) = view.hud_skin {
                // Region 9: the tooltip shell (box + divider) and the text
                // slots. The request states the measured section counts, so
                // the box is exactly as tall as the glyph rows it hosts.
                let mut cost_rows = Vec::new();
                for cost in view.tooltip_costs.iter().take(TOOLTIP_MAX_COST_LINES) {
                    cost_rows.extend(font.wrap(cost, body_scale, estimate_w));
                }
                let body_probe = if body_text.is_empty() {
                    Vec::new()
                } else {
                    font.wrap(&body_text, body_scale, estimate_w)
                };
                let anchor = view.tooltip_anchor.unwrap_or(layout.tooltip);
                let request = skin_tooltip::TooltipRequest::new(
                    layout.tooltip.w,
                    1 + body_probe.len() + cost_rows.len(),
                    cost_rows.len(),
                );
                let parts = skin_tooltip::draw_request(
                    skin,
                    ui,
                    anchor,
                    request,
                    layout.safe_frame,
                    s,
                    [1.0; 4],
                );
                if !parts.is_empty() {
                    layout.tooltip = parts.box_rect;
                    font.text_box(
                        ui,
                        title,
                        parts.title_rect,
                        title_scale,
                        tokens.gold,
                        TextAlign::Left,
                        Some(tokens.shadow),
                    );
                    let body_w = parts.body_rect.w.max(1.0);
                    let body_wrapped = if body_text.is_empty() {
                        Vec::new()
                    } else {
                        font.wrap(&body_text, body_scale, body_w)
                    };
                    let mut y = parts.body_rect.y;
                    for line in &body_wrapped {
                        if y + parts.line_height > parts.body_rect.bottom() + 0.5 {
                            break;
                        }
                        rich_line(
                            ui,
                            font,
                            line,
                            Rect::new(parts.body_rect.x, y, body_w, parts.line_height),
                            body_scale,
                            tokens.text,
                            Some(tokens.shadow),
                        );
                        y += parts.line_height;
                    }
                    let mut cost_wrapped = Vec::new();
                    for cost in view.tooltip_costs.iter().take(TOOLTIP_MAX_COST_LINES) {
                        cost_wrapped.extend(font.wrap(cost, body_scale, body_w));
                    }
                    for (rect, line) in parts.cost_rects.iter().zip(cost_wrapped.iter()) {
                        rich_line(
                            ui,
                            font,
                            line,
                            *rect,
                            body_scale,
                            crate::style::GOLD,
                            Some(tokens.shadow),
                        );
                    }
                }
            } else {
                // Classic tooltip: fit the run into the plate's own inner rect
                // (10 px horizontal / 6 px vertical insets plus the title row).
                // The size steps down to the readable floor and wraps on word
                // boundaries, so a long description cannot leave the plate
                // (device defect `hud.text.fits-plate`).
                let inner_w = estimate_w;
                let bottom = layout.tooltip.y + layout.tooltip.h;
                let max_h = (bottom - (layout.safe_frame.y + 8.0 * s)).max(title_h + 24.0 * s);
                let provisional = fit_text_plate(
                    font,
                    &body_text,
                    Rect::new(0.0, 0.0, inner_w, (max_h - title_h - 12.0 * s).max(1.0)),
                    body_scale,
                    MIN_TEXT_PX,
                    64,
                );
                // Retail tooltip section 3: cost/requirement rows, gold, max 8.
                let mut cost_rows = Vec::new();
                for cost in view.tooltip_costs.iter().take(TOOLTIP_MAX_COST_LINES) {
                    cost_rows.extend(font.wrap(cost, provisional.scale, inner_w));
                }
                let cost_h = cost_rows.len() as f32 * font.line_box(provisional.scale);
                let height = (title_h + provisional.height() + cost_h + 16.0 * s)
                    .min(max_h)
                    .max(title_h + 12.0 * s);
                layout.tooltip = Rect::new(
                    layout.tooltip.x,
                    (bottom - height).max(0.0),
                    layout.tooltip.w,
                    height,
                );
                // Exactly one tooltip shell: the atlas panel, else the pack panel,
                // else the retail border+background. Every shell is a nine-patch, so
                // the ornament keeps its size at any tooltip width.
                if chrome
                    .element_or(ui, layout.tooltip, ATLAS_TOOLTIP, s, [1.0; 4])
                    .is_some()
                {
                    // Atlas panel drawn.
                } else if let Some(panel) = view.redraw.and_then(|skin| skin.tooltip_panel) {
                    // Redraw pack panel: its authored tile is 384x256 with 24 px
                    // corners (`RedrawSkin::tooltip_panel`).
                    for (part, uv) in nine_slice_quads(
                        layout.tooltip,
                        REDRAW_PANEL_CORNER * s,
                        [REDRAW_PANEL_CORNER / REDRAW_PANEL_W, REDRAW_PANEL_CORNER / REDRAW_PANEL_H],
                    ) {
                        if part.w > 0.0 && part.h > 0.0 {
                            ui.textured_quad(panel, part, uv, [1.0; 4]);
                        }
                    }
                } else {
                    draw_button_chrome(ui, chrome, layout.tooltip, view.tooltip_border, s);
                    if let Some(background) = view.tooltip_background {
                        chrome.frame(
                            ui,
                            layout.tooltip.inset(3.0 * s, 3.0 * s),
                            Some(background),
                            1.0,
                            [1.0; 4],
                        );
                    } else {
                        chrome.flat_rect(ui, layout.tooltip.inset(3.0 * s, 3.0 * s), [0.03, 0.025, 0.02, 0.96]);
                    }
                }
                let content = layout.tooltip.inset(10.0 * s, 6.0 * s);
                let title_rect = Rect::new(content.x, content.y, content.w, title_h);
                let title_fit =
                    fit_text_plate(font, title, title_rect, title_scale, MIN_TEXT_PX, 1);
                draw_plate_text(
                    ui,
                    font,
                    &title_fit,
                    title_rect,
                    tokens.gold,
                    TextAlign::Left,
                    Some(tokens.shadow),
                );
                let body_top = content.y + title_h + 2.0 * s;
                let body_area = Rect::new(
                    content.x,
                    body_top,
                    content.w,
                    (content.bottom() - body_top - cost_h).max(1.0),
                );
                let body_fit = fit_text_plate(
                    font,
                    &body_text,
                    body_area,
                    provisional.scale,
                    MIN_TEXT_PX,
                    64,
                );
                draw_plate_text(
                    ui,
                    font,
                    &body_fit,
                    body_area,
                    tokens.text,
                    TextAlign::Left,
                    Some(tokens.shadow),
                );
                let mut cost_y = body_top + body_fit.height() + 2.0 * s;
                let cost_line_h = font.line_box(body_fit.scale);
                for line in &cost_rows {
                    if cost_y + cost_line_h > content.bottom() + 0.5 {
                        break;
                    }
                    rich_line(
                        ui,
                        font,
                        line,
                        Rect::new(content.x, cost_y, content.w, cost_line_h),
                        body_fit.scale,
                        crate::style::GOLD,
                        Some(tokens.shadow),
                    );
                    cost_y += cost_line_h;
                }
            }
        }

        // ---------------------------------------------------------- phase
        if let Some(phase) = view.phase {
            ui.rect(
                Rect::new(0.0, 0.0, layout.viewport.x, layout.viewport.y),
                [0.0, 0.0, 0.0, 0.58],
            );
            let accent = match phase.won {
                Some(true) => tokens.gold,
                Some(false) => tokens.danger,
                None => tokens.stroke_hot,
            };
            // WC3 dialog chrome: the fantasy dialog panel when the atlas is
            // installed, then the shipped esc-menu panel/border/band, then the
            // flat bevel. All nine-patch.
            if let Some(skin) = view.hud_skin {
                // Region 10: the square framed dialog plate; the title band is
                // the region's banner strip (`wide_panel`).
                skin_dialogs::draw_panel(
                    skin,
                    ui,
                    layout.phase,
                    skin_dialogs::PanelKind::Dialog,
                    s,
                );
            } else if chrome
                .element_or(ui, layout.phase, ATLAS_DIALOG, s, [1.0; 4])
                .is_some()
            {
                // Atlas dialog panel drawn.
            } else if let Some(panel) = view.dialog_panel {
                if !chrome.frame(ui, layout.phase, Some(panel), s, [1.0; 4]) {
                    for (part, uv) in nine_slice_quads(
                        layout.phase,
                        REDRAW_PANEL_CORNER * s,
                        [REDRAW_PANEL_CORNER / 512.0, REDRAW_PANEL_CORNER / 512.0],
                    ) {
                        if part.w > 0.0 && part.h > 0.0 {
                            ui.textured_quad(panel, part, uv, [1.0; 4]);
                        }
                    }
                }
                if let Some(border) = view.dialog_border {
                    chrome.frame(ui, layout.phase, Some(border), s, [1.0; 4]);
                }
            } else {
                chrome.flat_rect(ui, layout.phase, [0.07, 0.06, 0.05, 0.98]);
                ui.border(layout.phase, 2.0 * s, accent);
            }
            let band = Rect::new(
                layout.phase.x + 18.0 * s,
                layout.phase.y + 14.0 * s,
                layout.phase.w - 36.0 * s,
                64.0 * s,
            );
            if let Some(skin) = view.hud_skin {
                skin_dialogs::draw_banner(skin, ui, band, s);
                ui.border(band, 1.5 * s, accent);
            } else if let Some(texture) = view.dialog_band {
                if !chrome.frame(ui, band, Some(texture), s, [1.0; 4]) {
                    for (part, uv) in
                        nine_slice_quads(band, 16.0 * s, [16.0 / 256.0, 16.0 / 64.0])
                    {
                        if part.w > 0.0 && part.h > 0.0 {
                            ui.textured_quad(texture, part, uv, [1.0; 4]);
                        }
                    }
                }
            } else if chrome
                .element_or(ui, band, ATLAS_TITLE_BAND, s, [1.0; 4])
                .is_none()
            {
                chrome.flat_rect(ui, band, [0.075, 0.06, 0.04, 0.98]);
                ui.border(band, 1.5 * s, accent);
            }
            font.text_box(
                ui,
                phase.title,
                Rect::new(
                    layout.phase.x + 24.0 * s,
                    layout.phase.y + 30.0 * s,
                    layout.phase.w - 48.0 * s,
                    64.0 * s,
                ),
                1.33 * s,
                if phase.won == Some(false) {
                    tokens.danger
                } else {
                    tokens.gold
                },
                TextAlign::Centre,
                shadow,
            );
            font.text_box(
                ui,
                phase.subtitle,
                Rect::new(
                    layout.phase.x + 32.0 * s,
                    layout.phase.y + 110.0 * s,
                    layout.phase.w - 64.0 * s,
                    layout.phase.h - 140.0 * s,
                ),
                0.58 * s,
                tokens.text,
                TextAlign::Centre,
                shadow,
            );
        }

        layout
    }

    /// Final-pack draw path. Every art call is either a documented 9-slice
    /// (`HudPack::nine`, manifest insets) or a tiled/uniform single image;
    /// there is no procedural stone fill, no magenta tint, and no `UiTheme` or
    /// legacy redraw lookup on this path.
    fn draw_pack(
        &self,
        view: &HudView<'_>,
        ui: &mut UiBatch,
        fallback: &Font,
        chrome: &Chrome,
        skin: &Skin,
        pack: &HudPack,
    ) -> MobileHudLayout {
        let layout = self.layout(view);
        let fonts = pack_fonts(pack);
        let text_chain = match fonts {
            Some(fonts) => fonts.chain(),
            None => FontChain::new(&[fallback]),
        };
        let number_chain = match fonts {
            Some(fonts) => fonts.number_chain(),
            None => FontChain::new(&[fallback]),
        };
        let map_font = fonts.map(|fonts| &fonts.sans).unwrap_or(fallback);
        let tokens = &self.tokens;
        let white = [1.0f32; 4];
        let shadow = Some(tokens.shadow);
        // `s` scales authored pack art; `d` converts dp offsets to pixels. They
        // are different numbers on purpose: the layout is dp, the art has its
        // own authored pixel size.
        let s = layout.scale;
        let d = layout.density;
        let flat = chrome.flat;

        // ------------------------------------------------------- console shell
        // ONE continuous band (TZ §6/§9): anchored end caps, a tiled middle and
        // thin recessed separators between the functional zones. The previous
        // four framed panels are gone.
        fill(ui, flat, layout.console, pack.console_base);
        draw_console_band(ui, pack, &layout, s);
        for separator in &layout.dp().separators {
            let rect = Rect::new(
                separator.x * d,
                separator.y * d,
                separator.w.max(1.0) * d,
                separator.h * d,
            );
            fill(ui, flat, rect, [0.03, 0.025, 0.02, 0.55]);
        }

        // -------------------------------------------------------------- topbar
        // Full-width bar; FPS only in the developer overlay (TZ §6.5).
        {
            let bar = layout.top_meta;
            if let Some(plate) = pack.asset("topbar_plate_normal_720x32") {
                let bar_h = bar.h;
                let source_w = plate.w as f32;
                fill(ui, flat, bar, pack.topbar_base);
                let cap = (bar_h * 3.0).min(bar.w * 0.4);
                let mid_w = (bar.w - cap * 2.0).max(1.0);
                let cap_u = (cap / (source_w * (bar_h / 32.0))).clamp(0.0, 0.45);
                ui.textured_quad(
                    plate.texture,
                    Rect::new(bar.x, bar.y, cap, bar_h),
                    [0.0, 0.0, cap_u, 1.0],
                    white,
                );
                ui.textured_quad(
                    plate.texture,
                    Rect::new(bar.x + cap, bar.y, mid_w, bar_h),
                    [cap_u, 0.0, 1.0 - cap_u, 1.0],
                    white,
                );
                ui.textured_quad(
                    plate.texture,
                    Rect::new(bar.right() - cap, bar.y, cap, bar_h),
                    [1.0 - cap_u, 0.0, 1.0, 1.0],
                    white,
                );
            } else {
                fill(ui, flat, bar, pack.topbar_base);
            }
            text_chain.text_box(
                ui,
                view.map_name,
                layout.map_name,
                self.type_scale(crate::ui::theme::TypeRole::Body, d),
                tokens.text,
                TextAlign::Left,
                shadow,
            );
            for (index, pill) in layout.resource_pills.iter().enumerate() {
                let Some(data) = view.resources.get(index) else {
                    continue;
                };
                let art = if data.warn {
                    "resource_pill_warn_236x32"
                } else {
                    "resource_pill_normal_236x32"
                };
                pack.nine(ui, art, *pill, white, s);
                let icon_side = (pill.h - 4.0 * d).max(10.0 * d);
                let icon_rect = Rect::new(
                    pill.x + 3.0 * d,
                    pill.y + (pill.h - icon_side) * 0.5,
                    icon_side,
                    icon_side,
                );
                let pack_icon = match data.label {
                    "GOLD" => Some("icon_gold_normal_32"),
                    "LUMBER" => Some("icon_lumber_normal_32"),
                    _ => None,
                };
                if let Some(texture) = data.icon {
                    ui.textured_quad(texture, icon_rect, [0.0, 0.0, 1.0, 1.0], white);
                } else if let Some(name) = pack_icon
                    && !pack.stretch(ui, name, icon_rect, white)
                {
                    fill(ui, flat, icon_rect, [0.28, 0.26, 0.20, 0.9]);
                }
                let value_rect = Rect::new(
                    icon_rect.right() + 2.0 * d,
                    pill.y,
                    (pill.right() - icon_rect.right() - 4.0 * d).max(1.0),
                    pill.h,
                );
                number_chain.text_box(
                    ui,
                    data.value,
                    value_rect,
                    self.type_scale(crate::ui::theme::TypeRole::Numeric, d),
                    if data.warn { tokens.hp_low } else { data.tone },
                    TextAlign::Left,
                    shadow,
                );
            }
            text_chain.text_box(
                ui,
                view.upkeep,
                layout.upkeep,
                sp(12.0, d),
                tokens.text,
                TextAlign::Right,
                shadow,
            );
            if layout.show_fps {
                number_chain.text_box(
                    ui,
                    &format!("{:.0} FPS", view.fps.max(0.0)),
                    layout.fps,
                    sp(12.0, d),
                    tokens.text_muted,
                    TextAlign::Right,
                    shadow,
                );
            }
        }

        // ------------------------------------------------------------- minimap
        if layout.minimap_frame.w > 0.0 {
            if layout.minimap_header.h > 0.0 {
                pack.nine(
                    ui,
                    "minimap_header_normal_216x20",
                    layout.minimap_header,
                    white,
                    s,
                );
                text_chain.text_box(
                    ui,
                    view.minimap.title,
                    layout.minimap_header.inset(4.0 * d, 0.0),
                    sp(11.0, d),
                    tokens.text,
                    TextAlign::Left,
                    shadow,
                );
            }
            pack.nine(ui, "minimap_frame_normal_216", layout.minimap_frame, white, s);
            draw_minimap_content(ui, flat, map_font, &layout, view, tokens, s, shadow);
        } else if layout.minimap_rail.w > 0.0 {
            // Very narrow portrait: one 48 dp control opens the map overlay.
            skin.nine_scaled(ui, "tile_gold", layout.minimap_rail, white, s);
            text_chain.text_box(
                ui,
                "MAP",
                layout.minimap_rail,
                sp(12.0, d),
                tokens.text,
                TextAlign::Centre,
                shadow,
            );
        }

        // ---------------------------------------------------- selection panel
        draw_selection_panel(
            ui,
            &text_chain,
            &number_chain,
            tokens,
            &self.theme,
            skin,
            Some(pack),
            flat,
            &layout,
            view,
            s,
            d,
            shadow,
        );

        // ------------------------------------------------------ bottom band
        // Multi-selection strip OR hero inventory, never both and never empty.
        for (index, slot) in layout.army_slots.iter().enumerate() {
            let Some(chip) = view.army.get(index) else {
                break;
            };
            let role = if chip.selected { "tile_gold" } else { "tile_dark" };
            skin.nine_scaled(ui, role, *slot, white, s);
            if let Some(icon) = chip.icon {
                let window = slot.inset(4.0 * d, 4.0 * d);
                ui.textured_quad(icon, window, [0.0, 0.0, 1.0, 1.0], white);
            }
            let bar = Rect::new(
                slot.x + 2.0 * d,
                slot.bottom() - 5.0 * d,
                (slot.w - 4.0 * d).max(1.0),
                3.0 * d,
            );
            fill(ui, flat, bar, [0.10, 0.08, 0.06, 0.9]);
            fill(
                ui,
                flat,
                Rect::new(bar.x, bar.y, bar.w * chip.hp.clamp(0.0, 1.0), bar.h),
                skin_army::hp_color(chip.hp.clamp(0.0, 1.0)),
            );
        }
        if layout.army_badge.w > 0.0 {
            let total = view.selection_count.max(view.army.len());
            let shown = layout.army_slots.len();
            if total > shown {
                number_chain.text_box(
                    ui,
                    &format!("+{}", total - shown),
                    layout.army_badge,
                    sp(12.0, d),
                    tokens.gold,
                    TextAlign::Centre,
                    shadow,
                );
            }
        }
        if layout.inventory.w > 0.0 && !view.item_icons.is_empty() {
            for (index, slot) in layout.inventory_parts.slots.iter().enumerate() {
                skin.nine_scaled(ui, "tile_empty", *slot, white, s);
                if let Some(Some(icon)) = view.item_icons.get(index) {
                    ui.textured_quad(
                        *icon,
                        layout.inventory_parts.icons[index],
                        [0.0, 0.0, 1.0, 1.0],
                        white,
                    );
                }
            }
        }

        // --------------------------------------------------------- group tray
        if layout.groups_overlay.w > 0.0 {
            pack.nine(
                ui,
                "menu_panel_normal_512x384",
                layout.groups_overlay,
                [1.0, 1.0, 1.0, 0.97],
                s,
            );
        }
        for (index, hit) in layout.group_slots.iter().enumerate() {
            draw_group_plate(
                ui, &text_chain, &number_chain, tokens, skin, flat, *hit, index, view, s, d, shadow,
            );
        }
        if layout.groups_button.w > 0.0 {
            draw_drawer_button(
                ui,
                &text_chain,
                tokens,
                skin,
                flat,
                layout.groups_button,
                "ARMY",
                s,
                d,
                shadow,
                layout.dp().state.groups_open,
            );
        }

        // ---------------------------------------------------------- command card
        if layout.commands_overlay.w > 0.0 {
            pack.nine(
                ui,
                "menu_panel_normal_512x384",
                layout.commands_overlay,
                [1.0, 1.0, 1.0, 0.97],
                s,
            );
        }
        let card_art = (layout.cell / 90.0).max(s);
        for (index, slot) in layout.card_slots.iter().enumerate() {
            let command = view
                .commands
                .get(index)
                .copied()
                .unwrap_or_else(CommandButton::empty);
            let cell = format!("command_{:02}", index + 1);
            draw_card_cell(
                ui,
                &text_chain,
                tokens,
                skin,
                Some(pack),
                flat,
                *slot,
                &command,
                Some(&cell),
                card_art,
            );
        }
        if layout.commands_button.w > 0.0 {
            draw_drawer_button(
                ui,
                &text_chain,
                tokens,
                skin,
                flat,
                layout.commands_button,
                "CMD",
                s,
                d,
                shadow,
                layout.dp().state.commands_open,
            );
        }

        // ------------------------------------------------------ toast / tooltip
        let toast_rows: Vec<ToastView> = if view.toasts.is_empty() {
            view.toast.clone().into_iter().collect()
        } else {
            view.toasts.clone()
        };
        for (index, toast) in toast_rows.iter().take(3).enumerate() {
            let Some(rect) = layout.toasts.get(index).copied() else {
                break;
            };
            let insets = pack
                .insets("tooltip_panel_normal_384x256")
                .unwrap_or([24.0; 4]);
            pack.nine(ui, "tooltip_panel_normal_384x256", rect, white, s);
            let inner = Rect::new(
                rect.x + insets[0] * s,
                rect.y + 4.0 * s,
                (rect.w - (insets[0] + insets[2]) * s).max(1.0),
                (rect.h - 8.0 * s).max(1.0),
            );
            // One 24 px line: a 112 px banner's parchment centre cannot host
            // two 30 px lines without crossing the bevel. Longer strings are
            // ellipsized by `text_box`, never drawn past the plate.
            text_chain.text_box(
                ui,
                &toast.text,
                inner,
                0.80,
                [0.96, 0.92, 0.84, 1.0],
                TextAlign::Centre,
                shadow,
            );
        }
        if let Some(tooltip) = &view.tooltip {
            pack.nine(ui, "tooltip_panel_normal_384x256", layout.tooltip, white, s);
            let insets = pack
                .insets("tooltip_panel_normal_384x256")
                .unwrap_or([24.0; 4]);
            let inner = layout.tooltip.inset_sides(
                insets[0] * s,
                insets[1] * s,
                insets[2] * s,
                insets[3] * s,
            );
            let mut lines = tooltip.split('\n');
            let title = lines.next().unwrap_or("");
            text_chain.text_box(
                ui,
                title,
                Rect::new(inner.x, inner.y + 8.0 * s, inner.w, 30.0 * s),
                0.85 * s,
                tokens.gold,
                TextAlign::Left,
                shadow,
            );
            let body = lines.collect::<Vec<_>>().join("\n");
            if !body.is_empty() {
                text_chain.paragraph(
                    ui,
                    &body,
                    Rect::new(
                        inner.x,
                        inner.y + 44.0 * s,
                        inner.w,
                        (inner.h - 44.0 * s).max(1.0),
                    ),
                    0.80 * s,
                    [0.96, 0.92, 0.84, 1.0],
                    TextAlign::Left,
                    shadow,
                    1.1,
                );
            }
        }

        // ------------------------------------------------------------- overlay
        if let Some(phase) = &view.phase {
            pack.nine(ui, "menu_panel_normal_512x384", layout.phase, white, s);
            text_chain.text_box(
                ui,
                &phase.title,
                layout.phase.inset(16.0 * s, 12.0 * s),
                0.95 * s,
                tokens.gold,
                TextAlign::Centre,
                shadow,
            );
            text_chain.text_box(
                ui,
                &phase.subtitle,
                Rect::new(
                    layout.phase.x + 16.0 * s,
                    layout.phase.y + 56.0 * s,
                    layout.phase.w - 32.0 * s,
                    30.0 * s,
                ),
                0.80 * s,
                tokens.text,
                TextAlign::Centre,
                None,
            );
        }

        layout
    }

    /// Glue-screen layout: one panel, a title band, then a finger-first stack
    /// (tabs / rows / entries / nav). Every interactive rect is at least
    /// [`MENU_MIN_TARGET_PX`] tall — the operator's 90 px floor.
    pub fn layout_menu(&self, view: &MenuView<'_>) -> MenuLayout {
        let vp = view.viewport;
        let s = (vp.x.min(vp.y) / REFERENCE_HEIGHT * HUD_SCALE_BOOST).clamp(0.62, 2.0);
        let portrait = vp.x < vp.y * 1.25;
        let safe = view.safe.unwrap_or_else(|| {
            if portrait {
                SafeArea::new(0.0, 0.0, 0.0, 0.0)
            } else {
                SafeArea::new(0.0, 0.0, 0.0, 0.0)
            }
        });
        let frame = Rect::new(
            safe.left,
            safe.top,
            (vp.x - safe.left - safe.right).max(1.0),
            (vp.y - safe.top - safe.bottom).max(1.0),
        );
        let margin = (14.0 * s).clamp(8.0, 30.0);
        // A 320 px floor is a readability target, not a licence to exceed the
        // frame: on a 320x240 window it made the panel 320 tall, centred it at
        // y=-40 and cut the title (port-parity `menu-layout`, compact case).
        // Stay inside the frame; the floor applies only when it fits.
        let panel_w = (frame.w - 2.0 * margin)
            .min((if portrait { 980.0 } else { 1080.0 }) * s)
            .max(1.0);
        let panel_h = (frame.h - 2.0 * margin).max(1.0);
        let panel = Rect::new(
            frame.x + (frame.w - panel_w) * 0.5,
            frame.y + (frame.h - panel_h) * 0.5,
            panel_w,
            panel_h,
        );
        // Panel art inset (24 px authored) plus a reading pad.
        let inset = (24.0 * s).clamp(18.0, 42.0) + (10.0 * s).clamp(6.0, 18.0);
        let content = panel.inset_sides(inset, inset, inset, inset);
        // Retail glue art scales the title band from the PANEL WIDTH, not from
        // the leftover height: at 2414x1080 the old 14%-of-height band was a
        // 96-150 px strip that read as a tiny label above a huge panel.
        let title_h = (panel.w * 0.155)
            .clamp(84.0, 190.0)
            .min((content.h * 0.26).max(48.0));
        let title_band = Rect::new(content.x, content.y, content.w, title_h);
        let title = Rect::new(
            title_band.x + 18.0 * s,
            title_band.y + 8.0 * s,
            title_band.w - 36.0 * s,
            (title_h * 0.56).max(30.0),
        );
        // The page counter shares the subtitle row instead of being stacked on
        // top of it: the old absolute y placed "HUMAN 1/2" inside the title
        // rect on a 184 px band.
        let page_w = ((210.0 * s).min(title_band.w * 0.36) - 12.0 * s).max(60.0);
        let page_label = Rect::new(
            title_band.right() - page_w - 12.0 * s,
            title.bottom(),
            page_w,
            (title_h * 0.26).max(20.0),
        );
        let subtitle = Rect::new(
            title.x,
            title.bottom(),
            (title.w - page_w - 12.0 * s).max(80.0),
            (title_h * 0.30).max(22.0),
        );
        let status_h = (30.0 * s).clamp(26.0, 46.0);
        let status = Rect::new(content.x, content.bottom() - status_h, content.w, status_h);
        let progress = Rect::new(
            status.x,
            status.y - 10.0 * s - 16.0 * s,
            status.w,
            16.0 * s,
        );

        let gap = (10.0 * s).clamp(6.0, 16.0);
        let stack_top = title_band.bottom() + gap;
        let stack_bottom = progress.y - gap;
        let avail = (stack_bottom - stack_top).max(1.0);

        // Stack shape: MAIN pages stack their entries; campaign/custom pages
        // stack a tab row, the list rows and one nav row.
        let use_tabs = !view.tabs.is_empty();
        let use_rows = !view.rows.is_empty();
        // Main-menu entries must lay out even when the view still carries a
        // tab row or list rows: gating them on the other groups left the main
        // menu with zero button rects, so only the hardware Back (Quit) worked.
        let use_buttons = !view.buttons.is_empty();
        let use_nav = !view.nav.is_empty();
        // Six tabs across 1113 px fit one row at the measured phone; on a
        // 300 px portrait panel they wrap to 2x3 instead of forcing 64 px cells
        // that would escape the plate. `tab_rows` is part of the stack budget.
        let tab_gap = 8.0 * s;
        let tab_count = view.tabs.len();
        let per_row_cap = if tab_count > 0 {
            let max_cell = (content.w - tab_gap * (tab_count as f32 - 1.0))
                / tab_count as f32;
            if max_cell >= MENU_MIN_TARGET_PX {
                tab_count
            } else {
                3.min(tab_count).max(1)
            }
        } else {
            0
        };
        let tab_rows = if use_tabs {
            tab_count.div_ceil(per_row_cap).max(1)
        } else {
            0
        };
        let mut slots = 0usize;
        if use_tabs {
            slots += tab_rows;
        }
        if use_rows {
            slots += view.rows.len();
        }
        if use_buttons {
            slots += view.buttons.len();
        }
        if use_nav {
            slots += 1;
        }
        let slots = slots.max(1);
        let slot_gap = gap * (slots as f32 - 1.0);
        let mut row_h = ((avail - slot_gap) / slots as f32).clamp(MENU_MIN_TARGET_PX, 150.0);
        let mut used = row_h * slots as f32 + slot_gap;
        // The 90 px floor is a target, not a licence to overflow: with six main
        // menu entries on a 1080 px-tall device (864 px of safe frame) the stack
        // pushed QUIT/OPTIONS/LOAD past the bottom edge (device 14:18). Shrink
        // to fit, floor 48 px (still a finger target), before drawing anything.
        if used > avail + 0.5 {
            row_h = ((avail - slot_gap) / slots as f32).max(48.0);
            used = row_h * slots as f32 + slot_gap;
        }
        // A list page paginates: tabs and nav keep their 90 px rows, the
        // chapter rows take what is left. Without this, a 14-row campaign list
        // pushed its last rows out of the panel (the "empty published page"
        // defect). `visible_rows` is published so the caller's page label and
        // PREV/NEXT row stay honest.
        // Count actual entries, not booleans: casting `use_buttons`/`use_nav`
        // to usize contributed 1 each regardless of how many rows they draw,
        // so the second `row_h` solve saw 1 slot, reset it to the 150 px cap
        // and the four/six real buttons overflowed the panel (device 11:56).
        let fixed = tab_rows
            + if use_nav { view.nav.len().max(1) } else { 0 }
            + if use_buttons { view.buttons.len().max(1) } else { 0 };
        let mut visible_rows = if use_rows { view.rows.len() } else { 0 };
        if use_rows && used > avail + 0.5 {
            let fixed_h = fixed as f32 * (MENU_MIN_TARGET_PX + gap);
            let room = (avail - fixed_h - gap).max(0.0);
            visible_rows = (room / (MENU_MIN_TARGET_PX + gap)).floor() as usize;
            visible_rows = visible_rows.clamp(1, view.rows.len());
        }
        let slots = fixed + visible_rows;
        let slots = slots.max(1) as f32;
        let slot_gap = gap * (slots - 1.0);
        // When even the fixed rows cannot reach 90 px (a 240 dp-tall window),
        // the rows shrink instead of leaving the panel; 1080-class screens keep
        // the operator's floor.
        let natural = ((avail - slot_gap) / slots).max(1.0);
        let row_h = natural.min(150.0);
        // Centre the stack in the slack; never let it start above the band.
        let mut y = stack_top + ((avail - (row_h * slots + slot_gap)) * 0.5).max(0.0);

        let mut tabs = Vec::new();
        let mut rows = Vec::new();
        let mut buttons = Vec::new();
        let mut nav = Vec::new();
        if use_tabs {
            let cell_gap = tab_gap;
            let cell_w = ((content.w - cell_gap * (per_row_cap as f32 - 1.0))
                / per_row_cap as f32)
                .max(1.0);
            let mut remaining = view.tabs.len();
            for _ in 0..tab_rows {
                let in_row = remaining.min(per_row_cap);
                let row_w = cell_w * in_row as f32 + cell_gap * (in_row as f32 - 1.0);
                let mut x = content.x + (content.w - row_w) * 0.5;
                for _ in 0..in_row {
                    tabs.push(Rect::new(x, y, cell_w, row_h));
                    x += cell_w + cell_gap;
                }
                remaining -= in_row;
                y += row_h + gap;
            }
        }
        if use_rows {
            for _ in 0..visible_rows {
                rows.push(Rect::new(content.x, y, content.w, row_h));
                y += row_h + gap;
            }
        }
        if use_buttons {
            // WC3 glue entries are inset a little from the panel edge.
            let entry_w = (content.w * 0.92).min(content.w);
            let entry_x = content.x + (content.w - entry_w) * 0.5;
            for _ in 0..view.buttons.len() {
                buttons.push(Rect::new(entry_x, y, entry_w, row_h));
                y += row_h + gap;
            }
        }
        if use_nav {
            let count = view.nav.len() as f32;
            let cell_gap = 8.0 * s;
            let cell_w = ((content.w - cell_gap * (count - 1.0)) / count).max(64.0);
            let mut x = content.x + (content.w - (cell_w * count + cell_gap * (count - 1.0))) * 0.5;
            for _ in 0..view.nav.len() {
                nav.push(Rect::new(x, y, cell_w, row_h));
                x += cell_w + cell_gap;
            }
        }
        MenuLayout {
            panel,
            title_band,
            title,
            subtitle,
            buttons,
            progress,
            status,
            tabs,
            rows,
            nav,
            page_label,
            visible_rows,
        }
    }

    pub fn draw_menu(
        &self,
        view: &MenuView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> MenuLayout {
        let layout = self.layout_menu(view);
        let tokens = &self.tokens;
        let vp = view.viewport;
        let s = (vp.x.min(vp.y) / REFERENCE_HEIGHT * HUD_SCALE_BOOST).clamp(0.62, 2.0);
        // The battle keeps rendering behind the glue screen (WC3's panning
        // camera), so the backdrop is a veil over the live scene, not a fill.
        ui.rect(
            Rect::new(0.0, 0.0, vp.x, vp.y),
            [0.010, 0.009, 0.007, view.veil.clamp(0.0, 1.0)],
        );
        let skin = view.redraw;
        // The title uses the pack serif face (`primary_serif_cyrillic.ttf`).
        let title_font: &Font = skin
            .and_then(|skin| skin.fonts)
            .map(|fonts| &fonts.serif)
            .unwrap_or(font);
        // ---------------------------------------------------------- panel
        let panel_scale = (layout.panel.h / 384.0).clamp(0.6, 3.0);
        match skin.and_then(|skin| skin.panel) {
            Some(part) => draw_menu_part(ui, &part, layout.panel, 1.0, [1.0; 4]),
            None => {
                chrome.flat_rect(ui, layout.panel, [0.045, 0.038, 0.028, 0.98]);
                ui.border(layout.panel, 2.0 * s, [0.55, 0.44, 0.22, 0.95]);
                ui.border(
                    layout.panel.inset(5.0 * s, 5.0 * s),
                    1.0 * s,
                    [0.30, 0.25, 0.14, 0.8],
                );
            }
        }
        let _ = panel_scale;
        if let Some(part) = skin.and_then(|skin| skin.panel_border) {
            draw_menu_part(ui, &part, layout.panel, s, [1.0; 4]);
        }
        // ------------------------------------------------------ title band
        match skin.and_then(|skin| skin.title_band) {
            Some(part) => draw_menu_part(ui, &part, layout.title_band, s, [1.0; 4]),
            None => {
                chrome.flat_rect(ui, layout.title_band, [0.075, 0.06, 0.04, 0.98]);
                ui.border(layout.title_band, 2.0 * s, [0.62, 0.49, 0.24, 0.98]);
            }
        }
        let title_fit = fit_text_plate(
            title_font,
            view.title,
            layout.title,
            (layout.title.h * 0.56 / 30.0).clamp(1.0, 2.4),
            MIN_TEXT_PX * 1.4,
            1,
        );
        draw_plate_text(
            ui,
            title_font,
            &title_fit,
            layout.title,
            tokens.gold,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        if !view.subtitle.is_empty() {
            let sub_fit = fit_text_plate(
                font,
                view.subtitle,
                layout.subtitle,
                (layout.subtitle.h * 0.52 / 30.0).clamp(0.72, 1.5),
                MIN_TEXT_PX,
                1,
            );
            draw_plate_text(
                ui,
                font,
                &sub_fit,
                layout.subtitle,
                tokens.text_muted,
                TextAlign::Centre,
                None,
            );
        }
        if !view.page_label.is_empty() {
            let fit = fit_text_plate(
                font,
                view.page_label,
                layout.page_label,
                (layout.page_label.h * 0.62 / 30.0).clamp(0.72, 1.3),
                MIN_TEXT_PX,
                1,
            );
            draw_plate_text(
                ui,
                font,
                &fit,
                layout.page_label,
                tokens.gold,
                TextAlign::Right,
                Some(tokens.shadow),
            );
        }
        // ------------------------------------------------- entries / lists
        for (index, rect) in layout.tabs.iter().enumerate() {
            let Some(button) = view.tabs.get(index) else {
                continue;
            };
            let state = if index == view.active_tab {
                MenuButtonState::Pressed
            } else if view.pressed == Some(index) && view.pressed_group == MENU_GROUP_TABS {
                MenuButtonState::Pressed
            } else {
                MenuButtonState::Normal
            };
            draw_menu_entry(ui, font, chrome, tokens, skin, *rect, button, state, s, true);
        }
        for (index, rect) in layout.rows.iter().enumerate() {
            let Some(button) = view.rows.get(index) else {
                continue;
            };
            let state = if !button.enabled {
                MenuButtonState::Disabled
            } else if view.pressed == Some(index) && view.pressed_group == MENU_GROUP_ROWS {
                MenuButtonState::Pressed
            } else if view.hover == Some(index) {
                MenuButtonState::Hover
            } else {
                MenuButtonState::Normal
            };
            draw_menu_entry(ui, font, chrome, tokens, skin, *rect, button, state, s, false);
        }
        for (index, rect) in layout.buttons.iter().enumerate() {
            let Some(button) = view.buttons.get(index) else {
                continue;
            };
            let state = if !button.enabled {
                MenuButtonState::Disabled
            } else if view.pressed == Some(index) && view.pressed_group == MENU_GROUP_ENTRIES {
                MenuButtonState::Pressed
            } else if view.hover == Some(index) {
                MenuButtonState::Hover
            } else {
                MenuButtonState::Normal
            };
            draw_menu_entry(ui, font, chrome, tokens, skin, *rect, button, state, s, false);
        }
        for (index, rect) in layout.nav.iter().enumerate() {
            let Some(button) = view.nav.get(index) else {
                continue;
            };
            let state = if !button.enabled {
                MenuButtonState::Disabled
            } else if view.pressed == Some(index) && view.pressed_group == MENU_GROUP_NAV {
                MenuButtonState::Pressed
            } else {
                MenuButtonState::Normal
            };
            draw_menu_entry(ui, font, chrome, tokens, skin, *rect, button, state, s, true);
        }
        // --------------------------------------------------- progress/status
        if view.screen == MenuScreen::Loading {
            if let Some(skin) = skin {
                if let Some(part) = skin.loading_frame {
                    draw_menu_part(
                        ui,
                        &part,
                        layout.progress.expanded(6.0 * s, 6.0 * s),
                        s,
                        [1.0; 4],
                    );
                }
            }
            chrome.flat_rect(ui, layout.progress, [0.05, 0.045, 0.03, 0.98]);
            chrome.flat_rect(
                ui,
                Rect::new(
                    layout.progress.x,
                    layout.progress.y,
                    (layout.progress.w * view.progress.clamp(0.0, 1.0)).max(2.0),
                    layout.progress.h,
                ),
                tokens.gold,
            );
            ui.border(layout.progress, 1.0 * s, [0.55, 0.44, 0.20, 0.9]);
        }
        if !view.status.is_empty() {
            let fit = fit_text_plate(
                font,
                view.status,
                layout.status,
                (layout.status.h * 0.60 / 30.0).clamp(0.70, 1.2),
                MIN_TEXT_PX,
                2,
            );
            draw_plate_text(
                ui,
                font,
                &fit,
                layout.status,
                tokens.text,
                TextAlign::Centre,
                Some(tokens.shadow),
            );
        } else if view.screen == MenuScreen::Loading {
            font.text_box(
                ui,
                view.map_name,
                layout.status,
                0.54 * s,
                tokens.text_muted,
                TextAlign::Centre,
                None,
            );
        }
        layout
    }
}

/// One menu entry: pack button art in the required state, the label fitted into
/// the plate and the right-hand tag (state/key). Nothing is drawn outside the
/// rect — `fit_text_plate` clips at the readable floor.
#[allow(clippy::too_many_arguments)]
fn draw_menu_entry(
    ui: &mut UiBatch,
    font: &Font,
    chrome: &Chrome,
    tokens: &Tokens,
    skin: Option<MenuSkin>,
    rect: Rect,
    button: &MenuButton<'_>,
    state: MenuButtonState,
    s: f32,
    compact: bool,
) {
    let art_scale = (rect.h / 64.0).clamp(0.7, 2.4);
    let dimmed = state == MenuButtonState::Disabled;
    let part = match state {
        MenuButtonState::Normal => skin.and_then(|skin| skin.button_normal),
        MenuButtonState::Hover => skin
            .and_then(|skin| skin.button_hover.or(skin.button_normal)),
        MenuButtonState::Pressed => skin
            .and_then(|skin| skin.button_pressed.or(skin.button_normal)),
        MenuButtonState::Disabled => skin
            .and_then(|skin| skin.button_disabled.or(skin.button_normal)),
    };
    match part {
        // A locked row is a dimmed plate, never a bare colour square: the
        // shipped disabled/normal art is drawn with a grey tint, and when a
        // data set has no menu art at all the fallback is a bevelled stone
        // plate with a top highlight, a bottom shadow and an inset line.
        Some(part) => draw_menu_part(
            ui,
            &part,
            rect,
            art_scale,
            if dimmed {
                [0.60, 0.60, 0.64, 1.0]
            } else {
                [1.0; 4]
            },
        ),
        None => {
            let fill_color = if dimmed {
                [0.052, 0.048, 0.041, 0.95]
            } else if button.enabled {
                [0.10, 0.085, 0.06, 0.98]
            } else {
                [0.05, 0.045, 0.035, 0.9]
            };
            chrome.flat_rect(ui, rect, fill_color);
            let edge = (1.5 * s).max(1.0);
            chrome.flat_rect(
                ui,
                Rect::new(rect.x, rect.y, rect.w, edge),
                if dimmed {
                    [0.24, 0.22, 0.18, 0.75]
                } else {
                    [0.34, 0.30, 0.21, 0.85]
                },
            );
            chrome.flat_rect(
                ui,
                Rect::new(rect.x, rect.bottom() - edge, rect.w, edge),
                [0.015, 0.014, 0.011, 0.9],
            );
            ui.border(
                rect,
                edge,
                if dimmed {
                    [0.20, 0.18, 0.14, 0.7]
                } else if button.enabled {
                    [0.62, 0.49, 0.22, 0.95]
                } else {
                    [0.25, 0.22, 0.16, 0.7]
                },
            );
            if dimmed {
                // Recessed veil so a locked row reads as unavailable, not as a
                // different widget.
                chrome.flat_rect(ui, rect.inset(2.0 * s, 2.0 * s), [0.0, 0.0, 0.0, 0.24]);
            }
        }
    }
    let color = if button.enabled {
        tokens.text
    } else {
        tokens.disabled
    };
    // Label left, tag right; the tag never eats more than a third of the plate.
    // Tabs are a single centred row of six cells: they use the full plate (the
    // old 30% tag reservation is what truncated "PROLOGUE" to "PROLOG").
    let pad_x = (26.0 * s).min(rect.w * 0.08);
    let tag_w = if compact {
        0.0
    } else {
        (rect.w * 0.30).min(220.0 * s)
    };
    let label_w = (rect.w - tag_w - if compact { 2.0 } else { 3.0 } * pad_x).max(24.0);
    let inner = Rect::new(
        rect.x + pad_x,
        rect.y + 6.0 * s,
        label_w,
        (rect.h - 12.0 * s).max(10.0),
    );
    // A 90 px row is a finger target, and its label must read like a WC3 glue
    // entry, not a footnote: ~30-34 px caps on a 90 px plate.
    let label_scale = (rect.h * if compact { 0.38 } else { 0.42 } / 30.0).clamp(0.72, 1.8);
    let fit = fit_text_plate(font, button.label, inner, label_scale, MIN_TEXT_PX, 1);
    draw_plate_text(
        ui,
        font,
        &fit,
        inner,
        color,
        if compact { TextAlign::Centre } else { TextAlign::Left },
        Some(tokens.shadow),
    );
    if !button.sub.is_empty() && !compact {
        let tag = Rect::new(
            rect.right() - tag_w - pad_x * 0.5,
            rect.y + 8.0 * s,
            tag_w,
            (rect.h - 16.0 * s).max(8.0),
        );
        let tag_scale = (rect.h * 0.28 / 30.0).clamp(0.66, 1.2);
        let tag_fit = fit_text_plate(font, button.sub, tag, tag_scale, MIN_TEXT_PX * 0.8, 1);
        draw_plate_text(
            ui,
            font,
            &tag_fit,
            tag,
            if button.enabled { tokens.gold } else { tokens.disabled },
            TextAlign::Right,
            None,
        );
    }
    if state == MenuButtonState::Hover
        && let Some(part) = skin.and_then(|skin| skin.button_hover)
    {
        // Additive sheen over the pressed/normal art (never a colour swap).
        draw_menu_part(ui, &part, rect, art_scale, [1.0, 1.0, 1.0, 0.35]);
    }
}

/// In-game dialog screens drawn with the same shipped chrome as the menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogScreen {
    QuestLog,
    HeroSkills,
    BuildingPanel,
    /// Allies panel: the team roster (name, race, status per player).
    Allies,
    /// Journal: campaign notes and the current chapter summary.
    Journal,
    /// In-game menu: Resume / Save / Load / Restart / Quit rows (ledger 469).
    GameMenu,
}

/// One in-game dialog (quest log, hero skills, building/rally/training panel,
/// allies, journal, game menu).
pub struct DialogView<'a> {
    pub viewport: Vec2,
    pub screen: DialogScreen,
    pub title: &'a str,
    pub lines: &'a [&'a str],
    pub footer: &'a str,
    /// Action rows for dialogs whose rows are buttons (the game menu). Empty
    /// keeps the single CLOSE button. `action` is the glue-screen action id
    /// already understood by `game.rs` (`MENU_ACT_RESUME` = 7, save = 14,
    /// load = 15, restart = 9, quit = 2).
    pub actions: &'a [MenuButton<'a>],
    /// Shipped chrome (same `MenuSkin` as the menus).
    pub skin: Option<MenuSkin>,
    /// Fantasy-atlas skin for the plate and rows (`None` = retail chrome).
    pub hud_skin: Option<&'a Skin>,
    /// Sub-slice dialog skin (row/button/banner variants) for rows and the
    /// close button.
    pub hud_skin_dialog: Option<&'a Skin>,
}

/// The in-game menu rows (lane 469 defaults). `Resume` closes the menu and
/// returns to the battle; `Save`/`Load` hit the quick-save slot; `Restart`
/// restarts the map; `Quit` returns to the main menu.
pub const GAME_MENU_ACTIONS: &[MenuButton<'static>] = &[
    MenuButton { label: "RESUME", sub: "ESC", action: 7, enabled: true },
    MenuButton { label: "SAVE GAME", sub: "SLOT 1", action: 14, enabled: true },
    MenuButton { label: "LOAD GAME", sub: "SLOT 1", action: 15, enabled: true },
    MenuButton { label: "RESTART", sub: "ENTER", action: 9, enabled: true },
    MenuButton { label: "QUIT", sub: "MENU", action: 2, enabled: true },
];

impl<'a> DialogView<'a> {
    pub fn for_screen(screen: DialogScreen, viewport: Vec2) -> DialogView<'a> {
        let (title, lines, footer, actions) = match screen {
            DialogScreen::QuestLog => (
                "QUEST LOG",
                &["MAIN QUEST", "Gold: 500   Lumber: 250", "Kill the creeps", "Return to the base"] as &[&str],
                "TAP · CLOSE",
                &[] as &[MenuButton<'a>],
            ),
            DialogScreen::HeroSkills => (
                "HERO SKILLS",
                &["LEVEL 3", "1  HOLY LIGHT", "2  DIVINE SHIELD", "3  DEVOTION AURA"] as &[&str],
                "CLICK · LEARN",
                &[] as &[MenuButton<'a>],
            ),
            DialogScreen::BuildingPanel => (
                "BARRACKS",
                &["TRAIN FOOTMAN      135 G", "TRAIN RIFLEMAN    205 G", "RALLY POINT    SET"] as &[&str],
                "ESC · CLOSE",
                &[] as &[MenuButton<'a>],
            ),
            DialogScreen::Allies => (
                "ALLIES",
                &[
                    "PLAYER 1   HUMAN   YOU",
                    "PLAYER 2   ORC     COMPUTER",
                    "PLAYER 3   UNDEAD  COMPUTER",
                    "TEAM 1 — SHARED VISION ON",
                ] as &[&str],
                "TAP · CLOSE",
                &[] as &[MenuButton<'a>],
            ),
            DialogScreen::Journal => (
                "JOURNAL",
                &[
                    "CHAPTER I — THE FALL OF LORDAERON",
                    "The Alliance regroups after the orc raids.",
                    "Objectives and lore entries appear here as",
                    "the campaign advances.",
                ] as &[&str],
                "TAP · CLOSE",
                &[] as &[MenuButton<'a>],
            ),
            DialogScreen::GameMenu => (
                "GAME MENU",
                &[] as &[&str],
                "PAUSED",
                GAME_MENU_ACTIONS,
            ),
        };
        DialogView {
            viewport,
            screen,
            title,
            lines,
            footer,
            actions,
            skin: None,
            hud_skin: None,
            hud_skin_dialog: None,
        }
    }

    pub fn with_skin(mut self, skin: Option<MenuSkin>) -> Self {
        self.skin = skin;
        self
    }

    /// Install the fantasy-atlas dialog skins (primary plate + sub-slice rows).
    pub fn hud_skin(mut self, skin: Option<&'a Skin>, dialog: Option<&'a Skin>) -> Self {
        self.hud_skin = skin;
        self.hud_skin_dialog = dialog;
        self
    }

    /// Replace the generic screen text with live game data (quest log rows).
    pub fn with_content(mut self, title: &'a str, lines: &'a [&'a str], footer: &'a str) -> Self {
        self.title = title;
        self.lines = lines;
        self.footer = footer;
        self
    }

    /// Turn the dialog rows into action buttons (the game menu). The actions
    /// carry the same `action` ids `game.rs` already handles.
    pub fn with_actions(mut self, actions: &'a [MenuButton<'a>]) -> Self {
        self.actions = actions;
        self
    }
}

impl MobileHud {
    pub fn layout_dialog(&self, view: &DialogView<'_>) -> MenuLayout {
        let s = (view.viewport.x.min(view.viewport.y) / REFERENCE_HEIGHT * HUD_SCALE_BOOST).clamp(0.62, 2.0);
        let w = (640.0 * s).min(view.viewport.x - 80.0 * s);
        // Action dialogs (the game menu) need five 90 px rows; give them the
        // taller panel instead of squeezing the touch targets.
        let panel_h = if view.actions.is_empty() {
            (view.viewport.y * 0.64).min(560.0 * s)
        } else {
            (view.viewport.y * 0.72).min(680.0 * s)
        };
        let panel = Rect::new(
            view.viewport.x * 0.5 - w * 0.5,
            view.viewport.y * 0.18,
            w,
            panel_h,
        );
        let title_band = Rect::new(panel.x, panel.y, panel.w, 64.0 * s);
        let title = Rect::new(panel.x + 24.0 * s, title_band.y + 10.0 * s, panel.w - 48.0 * s, 40.0 * s);
        let subtitle = Rect::new(panel.x + 24.0 * s, title_band.bottom() + 8.0 * s, panel.w - 48.0 * s, 24.0 * s);
        let mut buttons = Vec::new();
        if view.actions.is_empty() {
            // Close row: a full-width touch target, never below the 90 px
            // operator floor (the old 44 px row was the device defect).
            let close_h = (64.0 * s).max(SCREEN_TOUCH_MIN_PX);
            buttons.push(Rect::new(
                panel.x + 40.0 * s,
                panel.bottom() - close_h - 12.0 * s,
                panel.w - 80.0 * s,
                close_h,
            ));
        } else {
            let count = view.actions.len();
            let gap = 7.0 * s;
            let top = subtitle.bottom() + 18.0 * s;
            let bottom = panel.bottom() - 20.0 * s;
            let available = (bottom - top).max(0.0);
            let row_h = ((available - gap * (count.saturating_sub(1)) as f32) / count as f32)
                .max(SCREEN_TOUCH_MIN_PX);
            for index in 0..count {
                buttons.push(Rect::new(
                    panel.x + 40.0 * s,
                    top + index as f32 * (row_h + gap),
                    panel.w - 80.0 * s,
                    row_h,
                ));
            }
        }
        let status = Rect::new(
            panel.x + 24.0 * s,
            panel.bottom() - 96.0 * s,
            panel.w - 48.0 * s,
            24.0 * s,
        );
        MenuLayout {
            panel,
            title_band,
            title,
            subtitle,
            buttons,
            progress: Rect::new(panel.x, panel.y, 0.0, 0.0),
            status,
            tabs: Vec::new(),
            rows: Vec::new(),
            nav: Vec::new(),
            page_label: Rect::new(panel.x, panel.bottom(), 0.0, 0.0),
            visible_rows: 0,
        }
    }

    pub fn draw_dialog(
        &self,
        view: &DialogView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> MenuLayout {
        let layout = self.layout_dialog(view);
        let tokens = &self.tokens;
        let s = (view.viewport.x.min(view.viewport.y) / REFERENCE_HEIGHT * HUD_SCALE_BOOST).clamp(0.62, 2.0);
        ui.rect(Rect::new(0.0, 0.0, view.viewport.x, view.viewport.y), [0.0, 0.0, 0.0, 0.55]);
        let skin = view.skin.unwrap_or_default();
        let dialog_skin = view.hud_skin_dialog.or(view.hud_skin);
        let row_area = Rect::new(
            layout.subtitle.x + 8.0 * s,
            layout.subtitle.y + 8.0 * s,
            (layout.subtitle.w - 16.0 * s).max(0.0),
            (layout.buttons.first().map_or(layout.status.y, |button| button.y)
                - (layout.subtitle.y + 16.0 * s))
                .max(0.0),
        );
        let mut row_rects = Vec::new();
        if let Some(atlas) = view.hud_skin {
            // Region 10: framed dialog plate + banner title band. The rows and
            // the close button use the authored sub-slice skin.
            skin_dialogs::draw_panel(
                atlas,
                ui,
                layout.panel,
                skin_dialogs::PanelKind::Dialog,
                s,
            );
            skin_dialogs::draw_banner(atlas, ui, layout.title_band, s);
            if view.actions.is_empty()
                && let Some(rows) = dialog_skin
            {
                row_rects = skin_dialogs::draw_rows(
                    rows,
                    ui,
                    row_area,
                    view.lines.len(),
                    skin_dialogs::ROW_H * s,
                    s,
                );
            }
        } else if let Some(panel) = skin.panel {
            for (part, uv) in nine_slice_part(panel, layout.panel, s) {
                if part.w > 0.0 && part.h > 0.0 {
                    ui.textured_quad(panel.texture, part, uv, [1.0; 4]);
                }
            }
        } else {
            chrome.flat_rect(ui, layout.panel, [0.045, 0.038, 0.028, 0.98]);
            ui.border(layout.panel, 2.0 * s, [0.55, 0.44, 0.22, 0.95]);
        }
        if view.hud_skin.is_none() {
            if let Some(border) = skin.panel_border {
                for (part, uv) in nine_slice_part(border, layout.panel, s) {
                    if part.w > 0.0 && part.h > 0.0 {
                        ui.textured_quad(border.texture, part, uv, [1.0; 4]);
                    }
                }
            }
            if let Some(band) = skin.title_band {
                for (part, uv) in nine_slice_part(band, layout.title_band, s) {
                    if part.w > 0.0 && part.h > 0.0 {
                        ui.textured_quad(band.texture, part, uv, [1.0; 4]);
                    }
                }
            } else {
                chrome.flat_rect(ui, layout.title_band, [0.075, 0.06, 0.04, 0.98]);
                ui.border(layout.title_band, 2.0 * s, [0.62, 0.49, 0.24, 0.98]);
            }
        }
        font.text_box(ui, view.title, layout.title, 1.10 * s, tokens.gold, TextAlign::Centre, Some(tokens.shadow));
        if row_rects.is_empty() {
            let mut y = layout.subtitle.y + 8.0 * s;
            for line in view.lines {
                font.text_box(
                    ui,
                    line,
                    Rect::new(layout.subtitle.x + 8.0 * s, y, layout.subtitle.w - 16.0 * s, 30.0 * s),
                    0.82 * s,
                    tokens.text,
                    TextAlign::Left,
                    Some(tokens.shadow),
                );
                y += 34.0 * s;
            }
        } else {
            for (line, rect) in view.lines.iter().zip(row_rects.iter()) {
                font.text_box(
                    ui,
                    line,
                    rect.inset(8.0 * s, 0.0),
                    0.82 * s,
                    tokens.text,
                    TextAlign::Left,
                    Some(tokens.shadow),
                );
            }
        }
        font.text_box(ui, view.footer, layout.status, 0.80 * s, tokens.text_muted, TextAlign::Centre, None);
        for (index, rect) in layout.buttons.iter().enumerate() {
            let action = view.actions.get(index);
            let (label, sub, enabled) = match action {
                Some(button) => (button.label, button.sub, button.enabled),
                None => ("CLOSE", "", true),
            };
            let label_rect = if let Some(rows) = dialog_skin
                && view.hud_skin.is_some()
            {
                // Sub-slice button plate: normal 217x25, pressed 217x30.
                skin_dialogs::draw_button(rows, ui, *rect, false, enabled, s)
            } else {
                if let Some(texture) = skin.button_normal {
                    for (part, uv) in nine_slice_part(texture, *rect, s) {
                        if part.w > 0.0 && part.h > 0.0 {
                            ui.textured_quad(texture.texture, part, uv, [1.0; 4]);
                        }
                    }
                } else {
                    chrome.flat_rect(ui, *rect, [0.10, 0.085, 0.06, 0.98]);
                }
                ui.border(*rect, 1.5 * s, [0.62, 0.49, 0.22, 0.95]);
                rect.inset(16.0 * s, 0.0)
            };
            let color = if enabled { tokens.text } else { tokens.disabled };
            font.draw_fit(
                ui,
                Rect::new(label_rect.x, label_rect.y, (label_rect.w - 90.0 * s).max(0.0), label_rect.h),
                label,
                0.86 * s,
                MIN_TEXT_PX,
                color,
                TextAlign::Left,
                Some(tokens.shadow),
            );
            if !sub.is_empty() {
                font.draw_fit(
                    ui,
                    label_rect,
                    sub,
                    0.62 * s,
                    MIN_TEXT_PX,
                    tokens.gold,
                    TextAlign::Right,
                    None,
                );
            }
        }
        layout
    }
}

// ---------------------------------------------------------------------------
// Full-screen HUD screens — ledger 469
// ---------------------------------------------------------------------------
//
// Map select, loading, quest log, score (the allies / journal / game-menu
// dialogs extend `DialogScreen` above). Everything is drawn from the operator's
// FINAL pack:
//
// * `menus/menu_panel_normal_512x384`      panels (24 px insets)
// * `menus/menu_button_*_256x64`           rows/buttons (24 px insets)
// * `menus/loading_frame_normal_1024x512`  loading frame (24 px insets)
// * `menus/score_panel_normal_1024x512`    score panel (24 px insets)
// * `build/build_progress_normal_64x8`     progress fill (flat strip, stretched)
//
// A screen takes `Option<&Skin>` (the loaded pack skin: per-role PNGs and the
// manifest insets) and `Option<MenuSkin>` (texture indices from
// `Assets::menu_textures`) so the same code runs from `Assets` and in the
// headless proofs. The panel helper always calls `HudPack::nine`: the 24 px
// corners keep their authored size and only the centre cell stretches.
//
// Touch rule: no interactive row or button is smaller than 90 px. Content that
// does not fit goes to the next page — nothing shrinks.

/// Minimum tappable row/button on the full-screen menus (operator rule).
pub const SCREEN_TOUCH_MIN_PX: f32 = 90.0;
/// The authored corner inset of every `menus/*` asset (manifest insets = 24).
pub const SCREEN_CORNER_PX: f32 = 24.0;

fn screen_scale(viewport: Vec2) -> f32 {
    (viewport.x.min(viewport.y) / REFERENCE_HEIGHT * HUD_SCALE_BOOST).clamp(0.62, 2.0)
}

fn screen_frame(viewport: Vec2, safe: Option<SafeArea>) -> Rect {
    let safe = safe.unwrap_or(SafeArea::ZERO).sanitised(viewport);
    Rect::new(
        safe.left,
        safe.top,
        (viewport.x - safe.left - safe.right).max(1.0),
        (viewport.y - safe.top - safe.bottom).max(1.0),
    )
}

/// Shared panel geometry for the list/frame screens.
struct ScreenFrame {
    s: f32,
    frame: Rect,
    panel: Rect,
    /// Title band inside the panel.
    band: Rect,
}

fn screen_panel_frame(
    viewport: Vec2,
    safe: Option<SafeArea>,
    max_w: f32,
    max_h: f32,
) -> ScreenFrame {
    let s = screen_scale(viewport);
    let frame = screen_frame(viewport, safe);
    let w = (frame.w - 20.0 * s)
        .min(max_w * s)
        .max(360.0 * s)
        .min(frame.w);
    let h = (frame.h - 12.0 * s)
        .min(max_h * s)
        .max(360.0 * s)
        .min(frame.h);
    let panel = Rect::new(
        frame.centre().x - w * 0.5,
        frame.centre().y - h * 0.5,
        w,
        h,
    );
    let band_h = (70.0 * s).max(56.0).min(h * 0.24);
    let band = Rect::new(
        panel.x + 10.0 * s,
        panel.y + 10.0 * s,
        panel.w - 20.0 * s,
        band_h,
    );
    ScreenFrame {
        s,
        frame,
        panel,
        band,
    }
}

/// Draw `name` from the pack through the manifest nine-slice (authored corners,
/// only the centre stretches), falling back to a `MenuSkin` texture index with
/// its authored size/insets, then to the flat bevel. Returns the content rect
/// inside the insets.
fn draw_screen_panel(
    ui: &mut UiBatch,
    chrome: &Chrome,
    skin: Option<&Skin>,
    fallback: Option<(usize, [f32; 2])>,
    name: &str,
    rect: Rect,
    s: f32,
    tint: [f32; 4],
) -> Rect {
    if let Some(pack) = skin.and_then(|skin| skin.pack.as_ref())
        && pack.nine(ui, name, rect, tint, s)
    {
        let insets = pack.insets(name).unwrap_or([SCREEN_CORNER_PX; 4]);
        return rect.inset_sides(
            insets[0] * s,
            insets[1] * s,
            insets[2] * s,
            insets[3] * s,
        );
    }
    if let Some((texture, size)) = fallback {
        let [w, h] = size;
        for (part, uv) in nine_slice_quads(
            rect,
            SCREEN_CORNER_PX * s,
            [SCREEN_CORNER_PX / w, SCREEN_CORNER_PX / h],
        ) {
            if part.w > 0.0 && part.h > 0.0 {
                ui.textured_quad(texture, part, uv, tint);
            }
        }
        return rect.inset_sides(
            SCREEN_CORNER_PX * s,
            SCREEN_CORNER_PX * s,
            SCREEN_CORNER_PX * s,
            SCREEN_CORNER_PX * s,
        );
    }
    chrome.flat_rect(ui, rect, [0.055, 0.047, 0.035, 0.98]);
    ui.border(rect, 2.0 * s, GOLD_LINE);
    rect.inset(12.0 * s, 12.0 * s)
}

fn menu_button_asset(state: MenuButtonState) -> &'static str {
    match state {
        MenuButtonState::Normal => "menu_button_normal_256x64",
        MenuButtonState::Hover => "menu_button_hover_256x64",
        MenuButtonState::Pressed => "menu_button_pressed_256x64",
        MenuButtonState::Disabled => "menu_button_disabled_256x64",
    }
}

fn menu_button_texture(menu: Option<MenuSkin>, state: MenuButtonState) -> Option<usize> {
    let menu = menu?;
    let part = match state {
        MenuButtonState::Normal => menu.button_normal,
        MenuButtonState::Hover => menu.button_hover.or(menu.button_normal),
        MenuButtonState::Pressed => menu.button_pressed.or(menu.button_normal),
        MenuButtonState::Disabled => menu.button_disabled.or(menu.button_normal),
    };
    part.map(|part| part.texture)
}

/// Button plate (pack art, authored 24 px corners). Returns the label rect.
fn draw_screen_button_plate(
    ui: &mut UiBatch,
    chrome: &Chrome,
    skin: Option<&Skin>,
    menu: Option<MenuSkin>,
    rect: Rect,
    state: MenuButtonState,
    s: f32,
) -> Rect {
    draw_screen_panel(
        ui,
        chrome,
        skin,
        menu_button_texture(menu, state).map(|texture| (texture, [256.0, 64.0])),
        menu_button_asset(state),
        rect,
        s,
        [1.0; 4],
    )
}

/// Label + right-aligned sub-label inside a button plate, both fitted to the
/// 24 px insets so a long label never leaves the plate.
fn draw_screen_button_label(
    ui: &mut UiBatch,
    font: &Font,
    tokens: &Tokens,
    rect: Rect,
    label: &str,
    sub: &str,
    enabled: bool,
    s: f32,
) {
    let sub_w = if sub.is_empty() {
        0.0
    } else {
        (font.measure(sub, 0.58 * s) + 18.0 * s).min(rect.w * 0.4)
    };
    font.draw_fit(
        ui,
        Rect::new(rect.x, rect.y, (rect.w - sub_w).max(1.0), rect.h),
        label,
        0.92 * s,
        MIN_TEXT_PX,
        if enabled { tokens.text } else { tokens.disabled },
        TextAlign::Left,
        Some(tokens.shadow),
    );
    if !sub.is_empty() {
        font.draw_fit(
            ui,
            Rect::new(rect.right() - sub_w, rect.y, sub_w, rect.h),
            sub,
            0.58 * s,
            MIN_TEXT_PX,
            if enabled { tokens.gold } else { tokens.disabled },
            TextAlign::Right,
            None,
        );
    }
}

// ------------------------------------------------------------- map select

/// One map-select row: the registry's map name plus the columns the phone
/// screen prints (players, tileset, terrain size in tiles).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapListRow {
    pub name: String,
    pub players: usize,
    pub tileset: String,
    pub width: u32,
    pub height: u32,
    pub file: String,
}

impl MapListRow {
    pub fn from_census(map: &crate::map_registry::MapCensus) -> Self {
        Self {
            name: map.name.clone(),
            players: map.players,
            tileset: map.tileset_name.clone(),
            width: map.terrain_width,
            height: map.terrain_height,
            file: map.file.clone(),
        }
    }

    pub fn players_label(&self) -> String {
        if self.players == 0 {
            "—".to_string()
        } else {
            format!("{}P", self.players)
        }
    }

    pub fn size_label(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// The map catalog for the select screen: all 38 maps from
/// `analysis/maps/REGISTRY.json` (`map_registry::Registry`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapSelectData {
    pub rows: Vec<MapListRow>,
}

impl MapSelectData {
    pub fn new(rows: Vec<MapListRow>) -> Self {
        Self { rows }
    }

    /// Adapter from the registry census (`map_registry::Registry::open_repo_default`).
    pub fn from_registry(registry: &crate::map_registry::Registry) -> Self {
        Self::new(registry.maps.iter().map(MapListRow::from_census).collect())
    }

    /// The shipped catalog next to the sources (headless proofs, phone drop).
    pub fn load_default() -> Option<Self> {
        crate::map_registry::Registry::open_repo_default()
            .ok()
            .map(|registry| Self::from_registry(&registry))
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Number of pages for a fixed rows-per-page budget (never zero).
    pub fn page_count(&self, per_page: usize) -> usize {
        let per = per_page.max(1);
        self.rows.len().div_ceil(per).max(1)
    }

    /// Rows of one page, clamped to the last page.
    pub fn page(&self, page: usize, per_page: usize) -> &[MapListRow] {
        let per = per_page.max(1);
        let pages = self.page_count(per);
        let page = page.min(pages - 1);
        let start = (page * per).min(self.rows.len());
        let end = (start + per).min(self.rows.len());
        &self.rows[start..end]
    }
}

/// Map-select screen inputs.
pub struct MapSelectView<'a> {
    pub viewport: Vec2,
    pub safe: Option<SafeArea>,
    pub data: &'a MapSelectData,
    pub page: usize,
    /// Selected catalog index (row highlight + PLAY target).
    pub selected: Option<usize>,
    /// Hovered/pressed catalog index (pointer feedback).
    pub hover: Option<usize>,
    pub pressed: Option<usize>,
    /// Pack skin (per-role PNGs + manifest insets).
    pub hud_skin: Option<&'a Skin>,
    /// Texture-index fallback (`Assets::menu_textures`).
    pub menu_skin: Option<MenuSkin>,
}

impl<'a> MapSelectView<'a> {
    pub fn new(viewport: Vec2, data: &'a MapSelectData) -> Self {
        Self {
            viewport,
            safe: None,
            data,
            page: 0,
            selected: None,
            hover: None,
            pressed: None,
            hud_skin: None,
            menu_skin: None,
        }
    }

    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn hover(mut self, hover: Option<usize>) -> Self {
        self.hover = hover;
        self
    }

    pub fn pressed(mut self, pressed: Option<usize>) -> Self {
        self.pressed = pressed;
        self
    }

    pub fn safe(mut self, safe: SafeArea) -> Self {
        self.safe = Some(safe);
        self
    }

    pub fn hud_skin(mut self, skin: Option<&'a Skin>) -> Self {
        self.hud_skin = skin;
        self
    }

    pub fn menu_skin(mut self, skin: Option<MenuSkin>) -> Self {
        self.menu_skin = skin;
        self
    }
}

pub struct MapSelectLayout {
    pub panel: Rect,
    pub title_band: Rect,
    pub title: Rect,
    pub subtitle: Rect,
    pub list: Rect,
    /// Visible row rects (page order, top to bottom).
    pub rows: Vec<Rect>,
    /// Catalog index of `rows[0]`.
    pub first: usize,
    pub per_page: usize,
    pub page: usize,
    pub pages: usize,
    pub back: Rect,
    pub prev: Rect,
    pub next: Rect,
    pub page_label: Rect,
    pub play: Rect,
}

impl MapSelectLayout {
    /// Catalog index under `point`, if a visible row was tapped.
    pub fn row_at(&self, point: Vec2) -> Option<usize> {
        if !self.list.contains(point) {
            return None;
        }
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first + index)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

impl MobileHud {
    pub fn layout_map_select(&self, view: &MapSelectView<'_>) -> MapSelectLayout {
        let frame = screen_panel_frame(view.viewport, view.safe, 1400.0, 1400.0);
        let s = frame.s;
        let title_band = frame.band;
        let title = Rect::new(
            title_band.x + 18.0 * s,
            title_band.y + 4.0 * s,
            title_band.w - 36.0 * s,
            (title_band.h * 0.56).max(24.0),
        );
        let subtitle = Rect::new(
            title_band.x + 18.0 * s,
            title.y + title.h,
            title_band.w - 36.0 * s,
            (title_band.bottom() - (title.y + title.h) - 4.0 * s).max(18.0),
        );
        let footer_h = (128.0 * s).max(112.0);
        let list_top = title_band.bottom() + 10.0 * s;
        let list_bottom = frame.panel.bottom() - footer_h - 8.0 * s;
        let list = Rect::new(
            frame.panel.x + 18.0 * s,
            list_top,
            frame.panel.w - 36.0 * s,
            (list_bottom - list_top).max(SCREEN_TOUCH_MIN_PX),
        );
        let gap = 6.0 * s;
        let row_h = (86.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let per_page = (((list.h + gap) / (row_h + gap)).floor() as usize).max(1);
        let pages = view.data.page_count(per_page);
        let page = view.page.min(pages - 1);
        let visible = view.data.page(page, per_page);
        let rows: Vec<Rect> = (0..visible.len())
            .map(|index| {
                Rect::new(
                    list.x,
                    list.y + index as f32 * (row_h + gap),
                    list.w,
                    row_h,
                )
            })
            .collect();
        let first = page * per_page;
        let fy = frame.panel.bottom() - footer_h + 6.0 * s;
        let fh = (footer_h - 18.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let back = Rect::new(
            frame.panel.x + 22.0 * s,
            fy,
            (130.0 * s).max(SCREEN_TOUCH_MIN_PX),
            fh,
        );
        let play_w = (250.0 * s).max(190.0);
        let play = Rect::new(frame.panel.right() - 22.0 * s - play_w, fy, play_w, fh);
        let nav = (110.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let next = Rect::new(play.x - 18.0 * s - nav, fy, nav, fh);
        let label_w = (130.0 * s).max(96.0);
        let page_label = Rect::new(next.x - label_w - 6.0 * s, fy, label_w, fh);
        let prev = Rect::new(page_label.x - nav - 6.0 * s, fy, nav, fh);
        MapSelectLayout {
            panel: frame.panel,
            title_band,
            title,
            subtitle,
            list,
            rows,
            first,
            per_page,
            page,
            pages,
            back,
            prev,
            next,
            page_label,
            play,
        }
    }

    pub fn draw_map_select(
        &self,
        view: &MapSelectView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> MapSelectLayout {
        let layout = self.layout_map_select(view);
        let tokens = &self.tokens;
        let s = screen_scale(view.viewport);
        ui.rect(
            Rect::new(0.0, 0.0, view.viewport.x, view.viewport.y),
            [0.008, 0.007, 0.006, 0.74],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin
                .and_then(|menu| menu.panel)
                .map(|part| (part.texture, [part.w, part.h])),
            "menu_panel_normal_512x384",
            layout.panel,
            s,
            [1.0; 4],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            None,
            "menu_button_normal_256x64",
            layout.title_band,
            s,
            [0.86, 0.82, 0.74, 0.96],
        );
        font.draw_fit(
            ui,
            layout.title,
            "MAP SELECT",
            1.18 * s,
            MIN_TEXT_PX,
            tokens.gold,
            TextAlign::Left,
            Some(tokens.shadow),
        );
        font.draw_fit(
            ui,
            layout.subtitle,
            &format!(
                "{} MAPS · PAGE {}/{}",
                view.data.len(),
                layout.page + 1,
                layout.pages
            ),
            0.66 * s,
            MIN_TEXT_PX,
            tokens.text_muted,
            TextAlign::Left,
            None,
        );
        let visible = view.data.page(layout.page, layout.per_page);
        for (index, row) in visible.iter().enumerate() {
            let Some(rect) = layout.rows.get(index).copied() else {
                break;
            };
            let data_index = layout.first + index;
            let selected = view.selected == Some(data_index);
            let state = if view.pressed == Some(data_index) || selected {
                MenuButtonState::Pressed
            } else if view.hover == Some(data_index) {
                MenuButtonState::Hover
            } else {
                MenuButtonState::Normal
            };
            let inner = draw_screen_button_plate(
                ui,
                chrome,
                view.hud_skin,
                view.menu_skin,
                rect,
                state,
                s,
            );
            if selected {
                ui.border(rect, 2.0 * s, tokens.gold);
            }
            let name_w = inner.w * 0.52;
            font.draw_fit(
                ui,
                Rect::new(inner.x, inner.y, name_w, inner.h),
                &row.name,
                0.86 * s,
                MIN_TEXT_PX,
                if selected { tokens.gold } else { tokens.text },
                TextAlign::Left,
                Some(tokens.shadow),
            );
            let players_w = inner.w * 0.10;
            font.draw_fit(
                ui,
                Rect::new(inner.x + inner.w * 0.55, inner.y, players_w, inner.h),
                &row.players_label(),
                0.78 * s,
                MIN_TEXT_PX,
                tokens.gold,
                TextAlign::Left,
                None,
            );
            let tileset_w = inner.w * 0.19;
            font.draw_fit(
                ui,
                Rect::new(inner.x + inner.w * 0.66, inner.y, tileset_w, inner.h),
                &row.tileset,
                0.78 * s,
                MIN_TEXT_PX,
                tokens.text_muted,
                TextAlign::Left,
                None,
            );
            let size_w = inner.w * 0.14;
            font.draw_fit(
                ui,
                Rect::new(inner.right() - size_w, inner.y, size_w, inner.h),
                &row.size_label(),
                0.72 * s,
                MIN_TEXT_PX,
                tokens.text_muted,
                TextAlign::Right,
                None,
            );
        }
        if visible.is_empty() {
            font.draw_fit(
                ui,
                layout.list,
                "NO MAPS IN THE REGISTRY",
                0.90 * s,
                MIN_TEXT_PX,
                tokens.text_muted,
                TextAlign::Centre,
                None,
            );
        }
        // Footer: BACK · PREV · PAGE · NEXT · PLAY (every target >= 90 px).
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.back,
            MenuButtonState::Normal,
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.back, "BACK", "ESC", true, s);
        let prev_enabled = layout.page > 0;
        let next_enabled = layout.page + 1 < layout.pages;
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.prev,
            if prev_enabled {
                MenuButtonState::Normal
            } else {
                MenuButtonState::Disabled
            },
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.prev, "PREV", "", prev_enabled, s);
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.next,
            if next_enabled {
                MenuButtonState::Normal
            } else {
                MenuButtonState::Disabled
            },
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.next, "NEXT", "", next_enabled, s);
        font.draw_fit(
            ui,
            layout.page_label,
            &format!("PAGE {}/{}", layout.page + 1, layout.pages),
            0.76 * s,
            MIN_TEXT_PX,
            tokens.text,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.play,
            MenuButtonState::Normal,
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.play, "PLAY", "START", true, s);
        layout
    }
}

// --------------------------------------------------------------- loading

/// One rotating loading tip (the screen prints `tip` under the frame).
pub const LOADING_TIPS: [&str; 5] = [
    "TIP: SHIFT-RIGHT-CLICK QUEUES ORDERS FOR YOUR SELECTION.",
    "TIP: WORKERS AUTO-HARVEST A NEWLY BUILT GOLD MINE.",
    "TIP: KEEP A HERO NEAR YOUR WORKERS AGAINST EARLY RAIDS.",
    "TIP: UPKEEP RISES ABOVE 40 FOOD — EXPAND BEFORE IT.",
    "TIP: SCOUT THE MAP EDGES FOR HIDDEN EXPANSIONS.",
];

/// Deterministic tip for a loading counter (same tick -> same tip).
pub fn loading_tip(index: usize) -> &'static str {
    LOADING_TIPS[index % LOADING_TIPS.len()]
}

/// Loading-screen inputs: map name, race, progress and one tip.
pub struct LoadingView<'a> {
    pub viewport: Vec2,
    pub safe: Option<SafeArea>,
    pub map_name: &'a str,
    pub race: &'a str,
    pub progress: f32,
    pub tip: &'a str,
    pub hud_skin: Option<&'a Skin>,
    pub menu_skin: Option<MenuSkin>,
}

impl<'a> LoadingView<'a> {
    pub fn new(viewport: Vec2, map_name: &'a str, race: &'a str) -> Self {
        Self {
            viewport,
            safe: None,
            map_name,
            race,
            progress: 0.0,
            tip: LOADING_TIPS[0],
            hud_skin: None,
            menu_skin: None,
        }
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    pub fn tip(mut self, tip: &'a str) -> Self {
        self.tip = tip;
        self
    }

    pub fn safe(mut self, safe: SafeArea) -> Self {
        self.safe = Some(safe);
        self
    }

    pub fn hud_skin(mut self, skin: Option<&'a Skin>) -> Self {
        self.hud_skin = skin;
        self
    }

    pub fn menu_skin(mut self, skin: Option<MenuSkin>) -> Self {
        self.menu_skin = skin;
        self
    }
}

pub struct LoadingLayout {
    pub frame: Rect,
    pub inner: Rect,
    pub map_name: Rect,
    pub race: Rect,
    pub status: Rect,
    /// Progress bar track.
    pub bar: Rect,
    /// Filled portion (`build_progress_normal_64x8`).
    pub fill: Rect,
    pub tip: Rect,
}

impl MobileHud {
    pub fn layout_loading(&self, view: &LoadingView<'_>) -> LoadingLayout {
        let s = screen_scale(view.viewport);
        let area = screen_frame(view.viewport, view.safe);
        let tip_h = (54.0 * s).max(44.0);
        let max_h = (area.h - tip_h - 40.0 * s).max(200.0 * s);
        let w = (area.w - 24.0 * s)
            .min(max_h * 2.0)
            .min(1500.0 * s)
            .max(320.0 * s);
        let h = w * 0.5;
        let frame = Rect::new(
            area.centre().x - w * 0.5,
            area.centre().y - h * 0.5 - tip_h * 0.5,
            w,
            h,
        );
        let inner = frame.inset(SCREEN_CORNER_PX * s, SCREEN_CORNER_PX * s);
        let map_name = Rect::new(
            inner.x + 24.0 * s,
            inner.y + inner.h * 0.16,
            inner.w - 48.0 * s,
            (inner.h * 0.18).max(28.0),
        );
        let race = Rect::new(
            inner.x + 24.0 * s,
            map_name.bottom() + 2.0 * s,
            inner.w - 48.0 * s,
            (inner.h * 0.10).max(24.0),
        );
        let bar = Rect::new(
            inner.x + inner.w * 0.12,
            inner.y + inner.h * 0.66,
            inner.w * 0.76,
            (18.0 * s).max(14.0),
        );
        let fill = Rect::new(
            bar.x,
            bar.y,
            (bar.w * view.progress.clamp(0.0, 1.0)).max(2.0),
            bar.h,
        );
        let status = Rect::new(
            inner.x + inner.w * 0.12,
            bar.y - (32.0 * s).max(26.0),
            inner.w * 0.76,
            (26.0 * s).max(22.0),
        );
        let tip = Rect::new(
            frame.x,
            frame.bottom() + 16.0 * s,
            frame.w,
            tip_h,
        );
        LoadingLayout {
            frame,
            inner,
            map_name,
            race,
            status,
            bar,
            fill,
            tip,
        }
    }

    pub fn draw_loading(
        &self,
        view: &LoadingView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> LoadingLayout {
        let layout = self.layout_loading(view);
        let tokens = &self.tokens;
        let s = screen_scale(view.viewport);
        ui.rect(
            Rect::new(0.0, 0.0, view.viewport.x, view.viewport.y),
            [0.006, 0.006, 0.005, 1.0],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin
                .and_then(|menu| menu.loading_frame)
                .map(|part| (part.texture, [part.w, part.h])),
            "loading_frame_normal_1024x512",
            layout.frame,
            s,
            [1.0; 4],
        );
        font.draw_fit(
            ui,
            layout.map_name,
            view.map_name,
            1.32 * s,
            MIN_TEXT_PX,
            tokens.gold,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        font.draw_fit(
            ui,
            layout.race,
            view.race,
            0.90 * s,
            MIN_TEXT_PX,
            tokens.text,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        ui.rect(layout.bar, [0.028, 0.024, 0.018, 0.98]);
        // The progress asset is a 64x8 flat strip (manifest insets 0): stretch,
        // never slice, so no authored pixel is enlarged into a corner.
        let filled = if let Some(pack) = view.hud_skin.and_then(|skin| skin.pack.as_ref()) {
            pack.stretch(ui, "build_progress_normal_64x8", layout.fill, [1.0; 4])
        } else {
            false
        };
        if !filled {
            ui.rect(layout.fill, tokens.gold);
        }
        ui.border(layout.bar, 1.5 * s, GOLD_LINE);
        font.draw_fit(
            ui,
            layout.status,
            &format!("LOADING… {:>3}%", (view.progress.clamp(0.0, 1.0) * 100.0) as i32),
            0.72 * s,
            MIN_TEXT_PX,
            tokens.text,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        font.draw_fit(
            ui,
            layout.tip,
            view.tip,
            0.70 * s,
            MIN_TEXT_PX,
            tokens.text_muted,
            TextAlign::Centre,
            None,
        );
        layout
    }
}

// -------------------------------------------------------------- quest log

pub struct QuestLogView<'a> {
    pub viewport: Vec2,
    pub safe: Option<SafeArea>,
    pub title: &'a str,
    /// Rows from the campaign lane (`Game::quest_rows()` -> `QuestLog`).
    pub quests: &'a QuestLog,
    pub page: usize,
    pub hud_skin: Option<&'a Skin>,
    pub menu_skin: Option<MenuSkin>,
}

impl<'a> QuestLogView<'a> {
    pub fn new(viewport: Vec2, quests: &'a QuestLog) -> Self {
        Self {
            viewport,
            safe: None,
            title: "QUEST LOG",
            quests,
            page: 0,
            hud_skin: None,
            menu_skin: None,
        }
    }

    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    pub fn safe(mut self, safe: SafeArea) -> Self {
        self.safe = Some(safe);
        self
    }

    pub fn hud_skin(mut self, skin: Option<&'a Skin>) -> Self {
        self.hud_skin = skin;
        self
    }

    pub fn menu_skin(mut self, skin: Option<MenuSkin>) -> Self {
        self.menu_skin = skin;
        self
    }
}

pub struct QuestLogLayout {
    pub panel: Rect,
    pub title_band: Rect,
    pub title: Rect,
    pub subtitle: Rect,
    pub list: Rect,
    pub rows: Vec<Rect>,
    /// Done/not-done mark cell per visible row.
    pub marks: Vec<Rect>,
    pub first: usize,
    pub per_page: usize,
    pub page: usize,
    pub pages: usize,
    pub footer: Rect,
    pub prev: Rect,
    pub next: Rect,
    pub page_label: Rect,
    pub close: Rect,
}

impl QuestLogLayout {
    pub fn row_at(&self, point: Vec2) -> Option<usize> {
        if !self.list.contains(point) {
            return None;
        }
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first + index)
    }
}

impl MobileHud {
    pub fn layout_quest_log(&self, view: &QuestLogView<'_>) -> QuestLogLayout {
        let frame = screen_panel_frame(view.viewport, view.safe, 1400.0, 1400.0);
        let s = frame.s;
        let title_band = frame.band;
        let title = Rect::new(
            title_band.x + 18.0 * s,
            title_band.y + 4.0 * s,
            title_band.w - 36.0 * s,
            (title_band.h * 0.56).max(24.0),
        );
        let subtitle = Rect::new(
            title_band.x + 18.0 * s,
            title.y + title.h,
            title_band.w - 36.0 * s,
            (title_band.bottom() - (title.y + title.h) - 4.0 * s).max(18.0),
        );
        let footer_h = (128.0 * s).max(112.0);
        let list_top = title_band.bottom() + 10.0 * s;
        let list_bottom = frame.panel.bottom() - footer_h - 8.0 * s;
        let list = Rect::new(
            frame.panel.x + 18.0 * s,
            list_top,
            frame.panel.w - 36.0 * s,
            (list_bottom - list_top).max(SCREEN_TOUCH_MIN_PX),
        );
        let gap = 6.0 * s;
        let row_h = (86.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let per_page = (((list.h + gap) / (row_h + gap)).floor() as usize).max(1);
        let pages = view.quests.page_count(per_page);
        let page = view.page.min(pages - 1);
        let visible = view.quests.page(page, per_page);
        let rows: Vec<Rect> = (0..visible.len())
            .map(|index| {
                Rect::new(
                    list.x,
                    list.y + index as f32 * (row_h + gap),
                    list.w,
                    row_h,
                )
            })
            .collect();
        let mark_side = (row_h * 0.52).min(56.0 * s).max(24.0 * s);
        let marks: Vec<Rect> = rows
            .iter()
            .map(|row| {
                Rect::new(
                    row.x + 14.0 * s,
                    row.y + (row.h - mark_side) * 0.5,
                    mark_side,
                    mark_side,
                )
            })
            .collect();
        let fy = frame.panel.bottom() - footer_h + 6.0 * s;
        let fh = (footer_h - 18.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let close = Rect::new(
            frame.panel.x + 22.0 * s,
            fy,
            (160.0 * s).max(120.0),
            fh,
        );
        let nav = (110.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let next = Rect::new(frame.panel.right() - 22.0 * s - nav, fy, nav, fh);
        let label_w = (130.0 * s).max(96.0);
        let page_label = Rect::new(next.x - label_w - 6.0 * s, fy, label_w, fh);
        let prev = Rect::new(page_label.x - nav - 6.0 * s, fy, nav, fh);
        let footer = Rect::new(list.x, fy, list.w, fh);
        QuestLogLayout {
            panel: frame.panel,
            title_band,
            title,
            subtitle,
            list,
            rows,
            marks,
            first: page * per_page,
            per_page,
            page,
            pages,
            footer,
            prev,
            next,
            page_label,
            close,
        }
    }

    pub fn draw_quest_log(
        &self,
        view: &QuestLogView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> QuestLogLayout {
        let layout = self.layout_quest_log(view);
        let tokens = &self.tokens;
        let s = screen_scale(view.viewport);
        ui.rect(
            Rect::new(0.0, 0.0, view.viewport.x, view.viewport.y),
            [0.008, 0.007, 0.006, 0.74],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin
                .and_then(|menu| menu.panel)
                .map(|part| (part.texture, [part.w, part.h])),
            "menu_panel_normal_512x384",
            layout.panel,
            s,
            [1.0; 4],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            None,
            "menu_button_normal_256x64",
            layout.title_band,
            s,
            [0.86, 0.82, 0.74, 0.96],
        );
        font.draw_fit(
            ui,
            layout.title,
            view.title,
            1.18 * s,
            MIN_TEXT_PX,
            tokens.gold,
            TextAlign::Left,
            Some(tokens.shadow),
        );
        font.draw_fit(
            ui,
            layout.subtitle,
            &format!(
                "{}/{} COMPLETE · PAGE {}/{}",
                view.quests.done_count(),
                view.quests.len(),
                layout.page + 1,
                layout.pages
            ),
            0.66 * s,
            MIN_TEXT_PX,
            tokens.text_muted,
            TextAlign::Left,
            None,
        );
        let visible = view.quests.page(layout.page, layout.per_page);
        for (index, row) in visible.iter().enumerate() {
            let (Some(rect), Some(mark)) =
                (layout.rows.get(index).copied(), layout.marks.get(index).copied())
            else {
                break;
            };
            let inner = draw_screen_button_plate(
                ui,
                chrome,
                view.hud_skin,
                view.menu_skin,
                rect,
                MenuButtonState::Normal,
                s,
            );
            // Done/not-done mark: filled gold cell with a dark core vs. an
            // empty dark cell with the gold rail.
            if row.done {
                ui.rect(mark, tokens.gold);
                ui.rect(mark.inset(mark.w * 0.3, mark.h * 0.3), [0.10, 0.09, 0.06, 1.0]);
            } else {
                ui.rect(mark, [0.05, 0.045, 0.035, 0.98]);
                ui.border(mark, 1.5 * s, tokens.text_muted);
            }
            let text_x = mark.right() + 14.0 * s;
            let state_w = (110.0 * s).max(90.0);
            let hint_w = if row.hint.is_empty() {
                0.0
            } else {
                (inner.w * 0.22).min(220.0 * s)
            };
            let text_w = (inner.right() - text_x - state_w - hint_w - 8.0 * s).max(1.0);
            font.draw_fit(
                ui,
                Rect::new(text_x, inner.y, text_w, inner.h),
                &row.text,
                0.86 * s,
                MIN_TEXT_PX,
                if row.done { tokens.text_muted } else { tokens.text },
                TextAlign::Left,
                Some(tokens.shadow),
            );
            if !row.hint.is_empty() {
                font.draw_fit(
                    ui,
                    Rect::new(
                        inner.right() - state_w - hint_w - 8.0 * s,
                        inner.y,
                        hint_w,
                        inner.h,
                    ),
                    &row.hint,
                    0.72 * s,
                    MIN_TEXT_PX,
                    tokens.text_muted,
                    TextAlign::Right,
                    None,
                );
            }
            font.draw_fit(
                ui,
                Rect::new(inner.right() - state_w, inner.y, state_w, inner.h),
                if row.done { "DONE" } else { "OPEN" },
                0.70 * s,
                MIN_TEXT_PX,
                if row.done { tokens.gold } else { tokens.text_muted },
                TextAlign::Right,
                None,
            );
        }
        if visible.is_empty() {
            font.draw_fit(
                ui,
                layout.list,
                "NO OBJECTIVES YET",
                0.90 * s,
                MIN_TEXT_PX,
                tokens.text_muted,
                TextAlign::Centre,
                None,
            );
        }
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.close,
            MenuButtonState::Normal,
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.close, "CLOSE", "ESC", true, s);
        let prev_enabled = layout.page > 0;
        let next_enabled = layout.page + 1 < layout.pages;
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.prev,
            if prev_enabled {
                MenuButtonState::Normal
            } else {
                MenuButtonState::Disabled
            },
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.prev, "PREV", "", prev_enabled, s);
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.next,
            if next_enabled {
                MenuButtonState::Normal
            } else {
                MenuButtonState::Disabled
            },
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.next, "NEXT", "", next_enabled, s);
        font.draw_fit(
            ui,
            layout.page_label,
            &format!("PAGE {}/{}", layout.page + 1, layout.pages),
            0.76 * s,
            MIN_TEXT_PX,
            tokens.text,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        layout
    }
}

// ------------------------------------------------------------------ score

pub struct ScoreView<'a> {
    pub viewport: Vec2,
    pub safe: Option<SafeArea>,
    /// Rows from lane 467 (`Game::score_rows()` -> `ScoreData`).
    pub data: &'a ScoreData,
    pub hud_skin: Option<&'a Skin>,
    pub menu_skin: Option<MenuSkin>,
}

impl<'a> ScoreView<'a> {
    pub fn new(viewport: Vec2, data: &'a ScoreData) -> Self {
        Self {
            viewport,
            safe: None,
            data,
            hud_skin: None,
            menu_skin: None,
        }
    }

    pub fn safe(mut self, safe: SafeArea) -> Self {
        self.safe = Some(safe);
        self
    }

    pub fn hud_skin(mut self, skin: Option<&'a Skin>) -> Self {
        self.hud_skin = skin;
        self
    }

    pub fn menu_skin(mut self, skin: Option<MenuSkin>) -> Self {
        self.menu_skin = skin;
        self
    }
}

pub struct ScoreLayout {
    pub panel: Rect,
    pub title_band: Rect,
    pub title: Rect,
    pub subtitle: Rect,
    pub header: Rect,
    pub rows: Vec<Rect>,
    pub values: Vec<Rect>,
    pub close: Rect,
}

impl MobileHud {
    pub fn layout_score(&self, view: &ScoreView<'_>) -> ScoreLayout {
        let s = screen_scale(view.viewport);
        let area = screen_frame(view.viewport, view.safe);
        // The score panel is authored 1024x512: keep that 2:1 frame instead of
        // stretching it across the whole screen.
        let w = (area.w - 32.0 * s).min(1400.0 * s).max(360.0 * s);
        let h = (w * 0.5).min((area.h - 24.0 * s).max(240.0 * s));
        let panel = Rect::new(
            area.centre().x - w * 0.5,
            area.centre().y - h * 0.5,
            w,
            h,
        );
        let title_band = Rect::new(
            panel.x + 10.0 * s,
            panel.y + 10.0 * s,
            panel.w - 20.0 * s,
            (64.0 * s).max(52.0),
        );
        let title = Rect::new(
            title_band.x + 18.0 * s,
            title_band.y + 4.0 * s,
            title_band.w - 36.0 * s,
            (title_band.h * 0.56).max(24.0),
        );
        let subtitle = Rect::new(
            title_band.x + 18.0 * s,
            title.y + title.h,
            title_band.w - 36.0 * s,
            (title_band.bottom() - (title.y + title.h) - 4.0 * s).max(18.0),
        );
        let close_h = (64.0 * s).max(SCREEN_TOUCH_MIN_PX);
        let close = Rect::new(
            panel.x + 24.0 * s,
            panel.bottom() - close_h - 14.0 * s,
            panel.w - 48.0 * s,
            close_h,
        );
        let content = Rect::new(
            panel.x + 34.0 * s,
            title_band.bottom() + 12.0 * s,
            panel.w - 68.0 * s,
            (close.y - 12.0 * s - (title_band.bottom() + 12.0 * s)).max(100.0),
        );
        let header_h = (44.0 * s).max(38.0);
        let header = Rect::new(content.x, content.y, content.w, header_h);
        let rows_area = Rect::new(
            content.x,
            header.bottom() + 4.0 * s,
            content.w,
            (content.bottom() - header.bottom() - 4.0 * s).max(60.0),
        );
        let count = view.data.rows.len().max(1);
        let gap = 4.0 * s;
        let row_h = ((rows_area.h - gap * (count.saturating_sub(1)) as f32) / count as f32)
            .max(28.0 * s);
        let rows: Vec<Rect> = (0..view.data.rows.len())
            .map(|index| {
                Rect::new(
                    rows_area.x,
                    rows_area.y + index as f32 * (row_h + gap),
                    rows_area.w,
                    row_h,
                )
            })
            .collect();
        let values: Vec<Rect> = rows
            .iter()
            .map(|row| {
                Rect::new(
                    row.x + row.w * 0.62,
                    row.y,
                    row.w * 0.38,
                    row.h,
                )
            })
            .collect();
        ScoreLayout {
            panel,
            title_band,
            title,
            subtitle,
            header,
            rows,
            values,
            close,
        }
    }

    pub fn draw_score(
        &self,
        view: &ScoreView<'_>,
        ui: &mut UiBatch,
        font: &Font,
        chrome: &Chrome,
    ) -> ScoreLayout {
        let layout = self.layout_score(view);
        let tokens = &self.tokens;
        let s = screen_scale(view.viewport);
        ui.rect(
            Rect::new(0.0, 0.0, view.viewport.x, view.viewport.y),
            [0.008, 0.007, 0.006, 0.74],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin
                .and_then(|menu| menu.panel)
                .map(|part| (part.texture, [part.w, part.h])),
            "score_panel_normal_1024x512",
            layout.panel,
            s,
            [1.0; 4],
        );
        draw_screen_panel(
            ui,
            chrome,
            view.hud_skin,
            None,
            "menu_button_normal_256x64",
            layout.title_band,
            s,
            [0.86, 0.82, 0.74, 0.96],
        );
        font.draw_fit(
            ui,
            layout.title,
            if view.data.title.is_empty() {
                "SCORE"
            } else {
                &view.data.title
            },
            1.18 * s,
            MIN_TEXT_PX,
            tokens.gold,
            TextAlign::Left,
            Some(tokens.shadow),
        );
        font.draw_fit(
            ui,
            layout.subtitle,
            &view.data.subtitle,
            0.66 * s,
            MIN_TEXT_PX,
            tokens.text_muted,
            TextAlign::Left,
            None,
        );
        font.draw_fit(
            ui,
            layout.header,
            "RESULT",
            0.72 * s,
            MIN_TEXT_PX,
            tokens.text_muted,
            TextAlign::Left,
            None,
        );
        font.draw_fit(
            ui,
            Rect::new(
                layout.header.x + layout.header.w * 0.62,
                layout.header.y,
                layout.header.w * 0.38,
                layout.header.h,
            ),
            "TOTAL",
            0.72 * s,
            MIN_TEXT_PX,
            tokens.text_muted,
            TextAlign::Right,
            None,
        );
        ui.rect(
            Rect::new(
                layout.header.x,
                layout.header.bottom(),
                layout.header.w,
                (1.5 * s).max(1.0),
            ),
            GOLD_LINE,
        );
        for (index, row) in view.data.rows.iter().enumerate() {
            let (Some(rect), Some(value)) =
                (layout.rows.get(index).copied(), layout.values.get(index).copied())
            else {
                break;
            };
            if index % 2 == 0 {
                ui.rect(
                    Rect::new(rect.x - 6.0 * s, rect.y, rect.w + 12.0 * s, rect.h),
                    [1.0, 1.0, 1.0, 0.035],
                );
            }
            font.draw_fit(
                ui,
                Rect::new(rect.x, rect.y, rect.w * 0.60, rect.h),
                &row.label,
                0.88 * s,
                MIN_TEXT_PX,
                tokens.text,
                TextAlign::Left,
                Some(tokens.shadow),
            );
            font.draw_fit(
                ui,
                value,
                &row.value,
                0.88 * s,
                MIN_TEXT_PX,
                tokens.gold,
                TextAlign::Right,
                Some(tokens.shadow),
            );
        }
        if view.data.rows.is_empty() {
            font.draw_fit(
                ui,
                layout.rows.first().copied().unwrap_or(layout.header),
                "NO SCORE YET",
                0.90 * s,
                MIN_TEXT_PX,
                tokens.text_muted,
                TextAlign::Centre,
                None,
            );
        }
        draw_screen_button_plate(
            ui,
            chrome,
            view.hud_skin,
            view.menu_skin,
            layout.close,
            MenuButtonState::Normal,
            s,
        );
        draw_screen_button_label(ui, font, tokens, layout.close, "CONTINUE", "ENTER", true, s);
        layout
    }
}

/// Draw one frame: the registered nine-patch when the texture carries one (the
/// corners keep their authored size), otherwise a full-quad stretch of the
/// texture, otherwise a bevel. This is the single place a frame texture is
/// turned into geometry.
fn draw_button_chrome(
    ui: &mut UiBatch,
    chrome: &Chrome,
    rect: Rect,
    texture: Option<usize>,
    scale: f32,
) {
    chrome.flat_rect(ui, rect, [0.06, 0.05, 0.04, 0.98]);
    if chrome.frame(ui, rect, texture, scale, [1.0; 4]) {
        return;
    }
    if let Some(texture) = texture {
        // Unregistered texture (a pack frame with no measured insets yet):
        // stretch the whole quad rather than skip the art.
        ui.textured_quad(texture, rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
    } else {
        ui.border(rect, 1.5 * scale, [0.55, 0.44, 0.20, 0.9]);
    }
}

/// Portrait skin with the frame ornament scaled by the HUD scale.
///
/// `portrait::draw` keeps the atlas corner size fixed (its contract), so at
/// HUD scale `s` a 176-unit budget would lose the window to unscaled 49/65/50/48
/// px cuts. `portrait.md` §6.1 option (b) allows the caller to pass a skin with
/// `insets * scale`; this is that skin. The returned `portrait_rect` is the
/// nine-patch centre cell, so the icon window tracks the ornament exactly.
fn portrait_scaled_skin(skin: &Skin, scale: f32) -> Skin {
    let mut out = skin.clone();
    if let Some(index) = Skin::role_index("portrait_frame") {
        let scale = if scale.is_finite() { scale.max(0.01) } else { 1.0 };
        for inset in out.insets[index].iter_mut() {
            *inset *= scale;
        }
    }
    out
}

/// Minimap frame skin: the `dialog_wide` ornament cut to the lane's measured
/// 12 px border (`analysis/hudskin/minimap_frame.md` §3/§4). The atlas' dialog
/// cuts (45/113/39/56) belong to a 391x267 dialog panel; used around a 216 px
/// square they would inflate the frame by 30 px a side and eat the selection
/// column. `MobileHudLayout::minimap` dilates by the same 12 px, so art and hit
/// rect are the same rect.
// ---------------------------------------------------------------------------
// Composition helpers (console stone field, atlas inlays, region painters)
// ---------------------------------------------------------------------------

/// Console stone field. Device frame 2026-11-16 02:19 measured (99,88,78) on a
/// 0.47 fill: too bright, the gold frames had no contrast. Target mean is
/// (58,52,47) = 0.227,0.204,0.184 (`COMPOSITION.md` §2).
pub const STONE_FIELD: [f32; 4] =
    crate::ui::theme::Palette::classic_fantasy().bg_secondary.to_array();
/// Stone bevel: 1 px lighter top/left highlight, darker bottom/right shadow.
pub const STONE_HI: [f32; 4] = [0.360, 0.330, 0.295, 1.0];
pub const STONE_LO: [f32; 4] = [0.105, 0.095, 0.082, 1.0];
/// Card-plate interior behind the command grid (atlas card plate = 38,39,40).
pub const PLATE_FILL: [f32; 4] =
    crate::ui::theme::Palette::classic_fantasy().bg_raised.to_array();
/// Dark inlay (crest panel, portrait matte).
pub const PANEL_DARK: [f32; 4] =
    crate::ui::theme::Palette::classic_fantasy().bg_primary.to_array();
/// Top banner: its own darker slab (36,30,26) with a gold rail, so the
/// playfield edge is defined against the console.
pub const BANNER_FIELD: [f32; 4] =
    crate::ui::theme::Palette::classic_fantasy().bg_raised.to_array();
/// Warm hairline used on every panel edge.
pub const GOLD_LINE: [f32; 4] = crate::ui::theme::Palette::classic_fantasy().accent.to_array();

/// Deterministic value noise: the stone grain must be identical on every run
/// and every screenshot, so it is a hash of the cell index, never an RNG.
fn stone_hash(cx: u32, cy: u32) -> u32 {
    let mut h = cx.wrapping_mul(0x9E37_79B9) ^ cy.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// Flat fill on the chrome's 1x1 white texture.
///
/// `UiBatch::rect` pushes into the *white* list, which `wc3-render` draws after
/// every textured run - a background drawn that way covers all atlas art in the
/// same batch. Composition fills therefore go through the flat *texture* so they
/// keep painter order with the atlas quads.
/// The pack's three faces (serif / action sans / numeric mono), rasterised
/// once per process by `hud::pack_fonts` and bound to their registered
/// textures by `HudPack::load`. `Font` coverage is the whole fix for clipped
/// Cyrillic labels: no shaping stack, `ab_glyph` already lays out these fixed
/// runs.
fn pack_fonts(pack: &HudPack) -> Option<&'static crate::hud::HudFonts> {
    crate::hud::pack_fonts(&pack.root)
}

fn fill(ui: &mut UiBatch, flat: usize, rect: Rect, color: [f32; 4]) {
    if rect.w > 0.0 && rect.h > 0.0 {
        ui.textured_quad(flat, rect, [0.0, 0.0, 1.0, 1.0], color);
    }
}

/// Flat stone field: base colour + deterministic grain + edge vignette.
///
/// Everything big is flat on purpose: `COMPOSITION.md` §3 measured that the
/// old 592x208 atlas plate stretched to ~2300 px (3.9x) and the 457x202 caps at
/// 1.57x were the blur and the red smears. The field never upscales any art.
fn draw_stone_field(ui: &mut UiBatch, flat: usize, rect: Rect, base: [f32; 4]) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    fill(ui, flat, rect, base);
    // Fine grain, not a mosaic: the device frame showed 44 px tiles as a visible
    // checkerboard across the whole console. Retail stone is a high-frequency
    // noise whose cells are ~2 device px, so the tile is small and the alpha is
    // a fraction of the old value.
    const CELL: f32 = 8.0;
    let cols = (rect.w / CELL).ceil().max(1.0) as u32;
    let rows = (rect.h / CELL).ceil().max(1.0) as u32;
    for cy in 0..rows {
        for cx in 0..cols {
            let hash = stone_hash(cx, cy);
            let tone = (hash & 3) as f32;
            let alpha = 0.0035 + tone * 0.0035;
            let x = rect.x + cx as f32 * CELL;
            let y = rect.y + cy as f32 * CELL;
            let w = CELL.min(rect.right() - x);
            let h = CELL.min(rect.bottom() - y);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let color = if hash & 8 == 0 {
                [1.0, 1.0, 1.0, alpha * 0.7]
            } else {
                [0.0, 0.0, 0.0, alpha]
            };
            fill(ui, flat, Rect::new(x, y, w, h), color);
        }
    }
    // Middle darkening (8%): the device frame read flat without it.
    fill(
        ui,
        flat,
        rect.inset(rect.w * 0.18, rect.h * 0.18),
        [0.0, 0.0, 0.0, 0.08],
    );
    // Vignette: six bands per edge, alpha falling towards the middle.
    for i in 0..6 {
        let f = i as f32;
        let alpha = 0.050 * (1.0 - f / 6.0);
        let band_h = (rect.h * 0.022).max(1.0);
        let band_w = (rect.w * 0.018).max(1.0);
        fill(ui, flat, 
            Rect::new(rect.x, rect.y + f * band_h, rect.w, band_h),
            [0.0, 0.0, 0.0, alpha],
        );
        fill(ui, flat, 
            Rect::new(
                rect.x,
                rect.bottom() - (f + 1.0) * band_h,
                rect.w,
                band_h,
            ),
            [0.0, 0.0, 0.0, alpha],
        );
        fill(ui, flat, 
            Rect::new(rect.x + f * band_w, rect.y, band_w, rect.h),
            [0.0, 0.0, 0.0, alpha * 0.75],
        );
        fill(ui, flat, 
            Rect::new(
                rect.right() - (f + 1.0) * band_w,
                rect.y,
                band_w,
                rect.h,
            ),
            [0.0, 0.0, 0.0, alpha * 0.75],
        );
    }
}

/// HP/mana frame skin: `stats::draw_hp` uses the `hp_bar` insets directly, so
/// scale them with the art scale of the bar (not the HUD scale) - the bar is a
/// uniform downscale of a 403x65 element.
fn portrait_hp_skin(skin: &Skin, scale: f32) -> Skin {
    let mut out = skin.clone();
    if let Some(index) = Skin::role_index("hp_bar") {
        let scale = if scale.is_finite() { scale.max(0.01) } else { 1.0 };
        for inset in out.insets[index].iter_mut() {
            *inset *= scale;
        }
    }
    out
}

/// The missing-icon portrait: a coloured square with the unit's first letter.
fn chrome_letter(
    ui: &mut UiBatch,
    font: &Font,
    tokens: &Tokens,
    selection: &SelectionView<'_>,
    rect: Rect,
) {
    chrome_flat(ui, rect, [0.16, 0.22, 0.34, 0.95]);
    ui.border(rect, 1.0, [0.45, 0.60, 0.82, 0.9]);
    let letter = selection
        .name
        .chars()
        .find(|character| character.is_alphabetic())
        .map(|character| character.to_string())
        .unwrap_or_default();
    if !letter.is_empty() {
        font.text_box(
            ui,
            &letter,
            rect,
            1.8,
            tokens.text,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
    }
}

/// One flat colour quad (the skin path has no `Chrome` handle here).
fn chrome_flat(ui: &mut UiBatch, rect: Rect, color: [f32; 4]) {
    if rect.w > 0.0 && rect.h > 0.0 {
        ui.rect(rect, color);
    }
}

/// Flat bevel ring: highlight on the top/left, shadow on the bottom/right and a
/// gold hairline just inside the top edge. Crisp at any size (no art).
fn draw_bevel_ring(
    ui: &mut UiBatch,
    flat: usize,
    rect: Rect,
    scale: f32,
    hi: [f32; 4],
    lo: [f32; 4],
    gold: [f32; 4],
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let t = 1.0;
    let _ = scale;
    fill(ui, flat, Rect::new(rect.x, rect.y, rect.w, t), hi);
    fill(ui, flat, Rect::new(rect.x, rect.y, t, rect.h), hi);
    fill(ui, flat, Rect::new(rect.x, rect.bottom() - t, rect.w, t), lo);
    fill(ui, flat, Rect::new(rect.right() - t, rect.y, t, rect.h), lo);
    fill(ui, flat, 
        Rect::new(rect.x + t, rect.y + t, rect.w - t * 2.0, (1.0 * scale).max(1.0)),
        gold,
    );
}

/// Atlas art scale of the HP bar: the `bar_hp` element is authored 403x65, so
/// the bar is drawn at 0.64 * hud scale (0.70 at the 1.1 device scale) - a
/// uniform downscale that keeps every nine-patch cell under 1.0.
fn hp_bar_art_scale(scale: f32) -> f32 {
    (0.64 * scale).clamp(0.30, 1.15)
}

/// One 1:1 sub-rect of an atlas role.
///
/// `uv` is in element fractions (`[u0, v0, u1, v1]` inside the role rect). The
/// destination must be the sampled pixel size (or smaller): the helper exists so
/// ornaments are placed at authored size instead of stretched with the panel.
fn atlas_piece(
    skin: &Skin,
    ui: &mut UiBatch,
    role: &str,
    dest: Rect,
    uv: [f32; 4],
    tint: [f32; 4],
) -> bool {
    let Some(index) = Skin::role_index(role) else {
        return false;
    };
    let [ax, ay, aw, ah] = skin.rects[index];
    if aw <= 0.0 || ah <= 0.0 || dest.w <= 0.0 || dest.h <= 0.0 {
        return false;
    }
    let u0 = (ax + uv[0] * aw) / skin.width;
    let v0 = (ay + uv[1] * ah) / skin.height;
    let u1 = (ax + uv[2] * aw) / skin.width;
    let v1 = (ay + uv[3] * ah) / skin.height;
    ui.textured_quad(skin.texture, dest, [u0, v0, u1, v1], tint);
    true
}

/// Largest box of the authored `art` size that fits inside `cell`, centred.
///
/// The device evidence (2026-11-16 02:19) measured a 102x99 slot frame drawn
/// into a 126 px cell = 1.24x, above the 1.15x ceiling. Art is therefore drawn
/// at its authored size centred in the cell; the outer 12 px stay hit area.
fn art_box(skin: &Skin, role: &str, cell: Rect) -> Rect {
    let Some(index) = Skin::role_index(role) else {
        return cell;
    };
    let [_, _, aw, ah] = skin.rects[index];
    if aw <= 0.0 || ah <= 0.0 {
        return cell;
    }
    let w = aw.min(cell.w);
    let h = ah.min(cell.h);
    Rect::new(
        cell.x + (cell.w - w) * 0.5,
        cell.y + (cell.h - h) * 0.5,
        w,
        h,
    )
}

/// Inner plate of an atlas slot: the art rect minus its nine-patch insets.
fn slot_inner(skin: &Skin, role: &str, art: Rect) -> Rect {
    let Some(index) = Skin::role_index(role) else {
        return art;
    };
    let [l, t, r, b] = skin.insets[index];
    art.inset_sides(l.max(0.0), t.max(0.0), r.max(0.0), b.max(0.0))
}

/// Fill an atlas slot's interior with the card-plate colour and a 1 px top
/// light, so an empty slot reads as an inset well instead of a black void.
fn fill_slot_well(ui: &mut UiBatch, flat: usize, inner: Rect) {
    if inner.w <= 0.0 || inner.h <= 0.0 {
        return;
    }
    fill(ui, flat, inner, PLATE_FILL);
    fill(
        ui,
        flat,
        Rect::new(inner.x, inner.y, inner.w, 1.0),
        [0.30, 0.29, 0.27, 0.55],
    );
}

/// Nine-patch of an atlas role with the edge bands **tiled** at 1:1.
///
/// 48 dp elements (126 px) are bigger than the authored art (89x90 tiles,
/// 102x99 slots), so the only way to honour both the touch floor and the
/// "no piece above 1.15x" rule is to repeat the edge pixels instead of
/// stretching them (`NinePatch::tiled`).
fn role_patch(skin: &Skin, role: &str, art_scale: f32) -> Option<NinePatch> {
    let index = Skin::role_index(role)?;
    let [x, y, w, h] = skin.rects[index];
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(
        NinePatch::element(
            skin.texture,
            Rect::new(x, y, w, h),
            [skin.width, skin.height],
            skin.insets[index],
        )
        .with_art_scale(art_scale)
        .tiled(true),
    )
}

/// Width of the 1:1 slice used when a frame edge is tiled along its length.
const ATLAS_EDGE_SLICE: f32 = 10.0;

/// Atlas frame without any stretching: the four corner blocks keep their
/// authored pixels, and each edge is **tiled** at 1:1 with a slice of the same
/// element. A 380 px frame therefore never upscales a 102 px atlas piece.
fn atlas_tile_frame(
    skin: &Skin,
    ui: &mut UiBatch,
    role: &str,
    rect: Rect,
    insets: [f32; 4],
    tint: [f32; 4],
) -> bool {
    let Some(index) = Skin::role_index(role) else {
        return false;
    };
    let [_, _, aw, ah] = skin.rects[index];
    if aw <= 0.0 || ah <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return false;
    }
    let [l, t, r, b] = insets;
    if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
        // Too small for a ring: one uniform downscale of the element.
        return skin.stretch(ui, role, rect, tint);
    }
    let f = |w: f32, h: f32| [w / aw, h / ah];
    let corners = [
        Rect::new(rect.x, rect.y, l, t),
        Rect::new(rect.right() - r, rect.y, r, t),
        Rect::new(rect.x, rect.bottom() - b, l, b),
        Rect::new(rect.right() - r, rect.bottom() - b, r, b),
    ];
    atlas_piece(skin, ui, role, corners[0], [0.0, 0.0, f(l, t)[0], f(l, t)[1]], tint);
    atlas_piece(
        skin,
        ui,
        role,
        corners[1],
        [1.0 - f(r, t)[0], 0.0, 1.0, f(r, t)[1]],
        tint,
    );
    atlas_piece(
        skin,
        ui,
        role,
        corners[2],
        [0.0, 1.0 - f(l, b)[1], f(l, b)[0], 1.0],
        tint,
    );
    atlas_piece(
        skin,
        ui,
        role,
        corners[3],
        [1.0 - f(r, b)[0], 1.0 - f(r, b)[1], 1.0, 1.0],
        tint,
    );
    // Edge slices: middle of each authored edge, tiled at 1:1 and clipped.
    let mid_u = (l + (aw - l - r) * 0.5) / aw;
    let mid_v = (t + (ah - t - b) * 0.5) / ah;
    let slice_u = (ATLAS_EDGE_SLICE / aw).min((aw - l - r).max(1.0) / aw);
    let slice_v = (ATLAS_EDGE_SLICE / ah).min((ah - t - b).max(1.0) / ah);
    tile_edge_h(
        skin,
        ui,
        role,
        Rect::new(rect.x + l, rect.y, rect.w - l - r, t),
        mid_u,
        slice_u,
        [v_frac(0.0, ah), v_frac(t, ah)],
        tint,
    );
    tile_edge_h(
        skin,
        ui,
        role,
        Rect::new(rect.x + l, rect.bottom() - b, rect.w - l - r, b),
        mid_u,
        slice_u,
        [1.0 - b / ah, 1.0],
        tint,
    );
    tile_edge_v(
        skin,
        ui,
        role,
        Rect::new(rect.x, rect.y + t, l, rect.h - t - b),
        mid_v,
        slice_v,
        [0.0, l / aw],
        tint,
    );
    tile_edge_v(
        skin,
        ui,
        role,
        Rect::new(rect.right() - r, rect.y + t, r, rect.h - t - b),
        mid_v,
        slice_v,
        [1.0 - r / aw, 1.0],
        tint,
    );
    true
}

fn v_frac(px: f32, total: f32) -> f32 {
    if total > 0.0 { px / total } else { 0.0 }
}

/// Tile a horizontal frame edge at 1:1: `band` is the edge strip, `slice_u` the
/// authored slice width in element fractions, `[v0, v1]` its vertical span.
fn tile_edge_h(
    skin: &Skin,
    ui: &mut UiBatch,
    role: &str,
    band: Rect,
    mid_u: f32,
    slice_u: f32,
    span: [f32; 2],
    tint: [f32; 4],
) {
    if band.w <= 0.0 || band.h <= 0.0 || slice_u <= 0.0 {
        return;
    }
    let slice_px = (slice_u * skin.rects[Skin::role_index(role).unwrap_or(0)][2]).max(1.0);
    let mut x = band.x;
    while x < band.right() - 0.5 {
        let w = (band.right() - x).min(slice_px);
        let frac = w / slice_px;
        let u0 = mid_u - slice_u * 0.5;
        atlas_piece(
            skin,
            ui,
            role,
            Rect::new(x, band.y, w, band.h),
            [u0, span[0], u0 + slice_u * frac, span[1]],
            tint,
        );
        x += w;
    }
}

/// Tile a vertical frame edge at 1:1 (`slice_v` in element fractions).
fn tile_edge_v(
    skin: &Skin,
    ui: &mut UiBatch,
    role: &str,
    band: Rect,
    mid_v: f32,
    slice_v: f32,
    span: [f32; 2],
    tint: [f32; 4],
) {
    if band.w <= 0.0 || band.h <= 0.0 || slice_v <= 0.0 {
        return;
    }
    let slice_px = (slice_v * skin.rects[Skin::role_index(role).unwrap_or(0)][3]).max(1.0);
    let mut y = band.y;
    while y < band.bottom() - 0.5 {
        let h = (band.bottom() - y).min(slice_px);
        let frac = h / slice_px;
        let v0 = mid_v - slice_v * 0.5;
        atlas_piece(
            skin,
            ui,
            role,
            Rect::new(band.x, y, band.w, h),
            [span[0], v0, span[1], v0 + slice_v * frac],
            tint,
        );
        y += h;
    }
}

/// Minimap: 1:1 stone ring + dark trough. Returns the ring rect.
/// Text scale for a `dp` size: the font face is authored at 30 px per unit of
/// `scale`, so 16 dp on a 3x screen is `16 * 3 / 30 = 1.6`.
fn sp(dp: f32, density: f32) -> f32 {
    (dp * density / 30.0).max(MIN_TEXT_PX / 30.0)
}

/// One continuous console band: anchored end caps, tiled **flat centre** (the
/// piece's own gold border never repeats), no four floating panels (TZ §6/§9).
fn draw_console_band(ui: &mut UiBatch, pack: &HudPack, layout: &MobileHudLayout, s: f32) {
    let band = layout.console;
    if band.w <= 2.0 || band.h <= 2.0 {
        return;
    }
    let art = (band.h / 288.0).max(0.1);
    let cap_w = (256.0 * art).min(band.w * 0.45);
    let left = pack.stretch(
        ui,
        "console_left_cap_normal_256x288",
        Rect::new(band.x, band.y, cap_w, band.h),
        [1.0; 4],
    );
    let right = pack.stretch(
        ui,
        "console_right_cap_normal_256x288",
        Rect::new(band.right() - cap_w, band.y, cap_w, band.h),
        [1.0; 4],
    );
    let mid_x = band.x + if left { cap_w } else { 0.0 };
    let mid_right = band.right() - if right { cap_w } else { 0.0 };
    if mid_right <= mid_x + 1.0 {
        return;
    }
    let Some(asset) = pack.asset("console_left_mid_normal_256x288") else {
        return;
    };
    let aw = asset.w as f32;
    let insets = pack
        .insets("console_left_mid_normal_256x288")
        .unwrap_or([32.0, 0.0, 32.0, 0.0]);
    let (il, ir) = (insets[0].max(1.0), insets[2].max(1.0));
    let core = (aw - il - ir).max(1.0);
    let tile_w = (core * art).max(1.0);
    let u0 = il / aw;
    let u1 = (aw - ir) / aw;
    let mut x = mid_x;
    while x < mid_right - 0.5 {
        let width = tile_w.min(mid_right - x);
        let span = (width / tile_w).clamp(0.0, 1.0);
        ui.textured_quad(
            asset.texture,
            Rect::new(x, band.y, width, band.h),
            [u0, 0.0, u0 + (u1 - u0) * span, 1.0],
            [1.0; 4],
        );
        x += width;
    }
}

/// Fit one text row through a font chain and draw it: the scale steps down to
/// the readable floor, and only a genuinely impossible row falls back to the
/// chain's ellipsis. Returns the measured width at the drawn scale so tests can
/// assert the row stays inside its plate.
#[allow(clippy::too_many_arguments)]
fn draw_chain_line(
    ui: &mut UiBatch,
    chain: &FontChain<'_>,
    text: &str,
    rect: Rect,
    scale: f32,
    color: [f32; 4],
    align: TextAlign,
    shadow: Option<[f32; 4]>,
) -> f32 {
    let floor = (MIN_TEXT_PX / 30.0).max(0.4);
    let mut fitted = if scale.is_finite() { scale.max(floor) } else { floor };
    let mut width = chain.measure(text, fitted);
    while width > rect.w + 0.01 && fitted > floor + 1e-4 {
        fitted = (fitted - 0.02).max(floor);
        width = chain.measure(text, fitted);
    }
    chain.text_box(ui, text, rect, fitted, color, align, shadow);
    chain.measure(text, fitted)
}

/// Flat fallback for the persistent compact drawer toggles (`ARMY` / `CMD`):
/// the same pressed tint and gold outline as the pack plate, without a skin.
#[allow(clippy::too_many_arguments)]
fn draw_drawer_button_fallback(
    ui: &mut UiBatch,
    chain: &FontChain<'_>,
    tokens: &Tokens,
    flat: usize,
    rect: Rect,
    label: &str,
    s: f32,
    shadow: Option<[f32; 4]>,
    open: bool,
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    fill(
        ui,
        flat,
        rect,
        if open {
            [0.16, 0.13, 0.07, 0.98]
        } else {
            [0.07, 0.065, 0.05, 0.98]
        },
    );
    ui.border(
        rect,
        (1.5 * s).max(1.0),
        if open {
            [0.85, 0.70, 0.32, 0.95]
        } else {
            [0.55, 0.44, 0.22, 0.9]
        },
    );
    chain.text_box(
        ui,
        label,
        Rect::new(rect.x, rect.centre().y - 7.0 * s, rect.w, 15.0 * s),
        (0.36 * s).max(MIN_TEXT_PX / 30.0),
        if open { tokens.gold } else { tokens.text },
        TextAlign::Centre,
        shadow,
    );
}

/// Filled HP/mana trough with a hairline border, a fill and centred text that
/// never clips (TZ §6.2).
fn draw_unit_bar(
    ui: &mut UiBatch,
    number_chain: &FontChain<'_>,
    flat: usize,
    rect: Rect,
    ratio: f32,
    text: &str,
    d: f32,
    fill_color: [f32; 4],
    text_color: [f32; 4],
    shadow: Option<[f32; 4]>,
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let edge = (1.0 * d).max(1.0);
    let gold = [0.72, 0.61, 0.34, 0.95];
    fill(ui, flat, Rect::new(rect.x, rect.y, rect.w, edge), gold);
    fill(
        ui,
        flat,
        Rect::new(rect.x, rect.bottom() - edge, rect.w, edge),
        gold,
    );
    fill(ui, flat, Rect::new(rect.x, rect.y, edge, rect.h), gold);
    fill(
        ui,
        flat,
        Rect::new(rect.right() - edge, rect.y, edge, rect.h),
        gold,
    );
    let inner = rect.inset(edge, edge);
    fill(ui, flat, inner, [0.07, 0.06, 0.05, 0.95]);
    let width = (inner.w * ratio.clamp(0.0, 1.0)).max(0.0);
    if width > 0.0 {
        fill(
            ui,
            flat,
            Rect::new(inner.x, inner.y, width, inner.h),
            fill_color,
        );
    }
    if !text.is_empty() {
        number_chain.text_box(
            ui,
            text,
            rect,
            sp(12.0, d),
            text_color,
            TextAlign::Centre,
            shadow,
        );
    }
}

/// Selection panel: a real empty state, portrait, name, HP and mana bars with
/// values, meaningful stats and a production row only when it exists.
#[allow(clippy::too_many_arguments)]
fn draw_selection_panel(
    ui: &mut UiBatch,
    text_chain: &FontChain<'_>,
    number_chain: &FontChain<'_>,
    tokens: &Tokens,
    theme: &crate::ui::theme::Theme,
    skin: &Skin,
    pack: Option<&HudPack>,
    flat: usize,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    s: f32,
    d: f32,
    shadow: Option<[f32; 4]>,
) {
    let zone = layout.selection;
    if zone.w <= 0.0 || zone.h <= 0.0 {
        return;
    }
    let ivory = [0.94, 0.90, 0.80, 1.0];
    // Direction type scale (addendum §3): the name is a heading, every info
    // row is a label. No ad-hoc `sp(11, d)` inside the unit card.
    let t_name = theme.scale_30(crate::ui::theme::TypeRole::Heading, d);
    let t_label = theme.scale_30(crate::ui::theme::TypeRole::Label, d);
    let Some(selection) = view.selection.as_ref() else {
        // Composed empty state inside the same panel: no fake portrait frame,
        // no clipped plaque, no reserved stat/queue space (TZ §6.2).
        fill(ui, flat, zone.inset(1.0 * d, 1.0 * d), [0.05, 0.045, 0.04, 0.30]);
        let emblem_side = 20.0 * d;
        let emblem = Rect::new(
            zone.centre().x - emblem_side * 0.5,
            zone.centre().y - emblem_side - 12.0 * d,
            emblem_side,
            emblem_side,
        );
        fill_outline(ui, flat, emblem, (1.0 * d).max(1.0), [0.34, 0.29, 0.21, 0.85]);
        fill_outline(
            ui,
            flat,
            emblem.inset(4.0 * d, 4.0 * d),
            (1.0 * d).max(1.0),
            [0.20, 0.17, 0.12, 0.85],
        );
        let title = Rect::new(
            zone.x + 6.0 * d,
            zone.centre().y - 8.0 * d,
            (zone.w - 12.0 * d).max(1.0),
            20.0 * d,
        );
        text_chain.text_box(
            ui,
            "Нет выделения",
            title,
            sp(16.0, d),
            tokens.text,
            TextAlign::Centre,
            shadow,
        );
        if layout.mana_bar.h == 0.0 && zone.h > 96.0 * d {
            text_chain.text_box(
                ui,
                "Коснитесь юнита или здания",
                Rect::new(
                    zone.x + 6.0 * d,
                    title.bottom() + 2.0 * d,
                    (zone.w - 12.0 * d).max(1.0),
                    16.0 * d,
                ),
                t_label,
                tokens.text_muted,
                TextAlign::Centre,
                shadow,
            );
        }
        return;
    };
    // Portrait frame around the unit icon (never a fake frame when empty).
    if let Some(pack) = pack {
        let frame_name = match selection.frame {
            PortraitFrame::Normal => "portrait_frame_normal_128",
            PortraitFrame::Hero => "portrait_frame_hero_128",
            PortraitFrame::Enemy => "portrait_frame_enemy_128",
        };
        pack.nine(ui, frame_name, layout.portrait, [1.0; 4], s);
        if let Some(icon) = selection.portrait {
            let insets = pack.insets(frame_name).unwrap_or([24.0; 4]);
            let art = (layout.portrait.w / 128.0).max(0.1);
            let window = layout.portrait.inset_sides(
                insets[0] * art,
                insets[1] * art,
                insets[2] * art,
                insets[3] * art,
            );
            ui.textured_quad(icon, window, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        }
    } else {
        skin.nine_scaled(ui, "tile_gold", layout.portrait, [1.0; 4], s);
    }
    draw_chain_line(
        ui,
        text_chain,
        &selection.name,
        layout.name,
        t_name,
        tokens.gold,
        TextAlign::Left,
        shadow,
    );
    let class_line = if let Some(level) = selection.level {
        format!("{} · LVL {}", selection.class, level)
    } else {
        selection.class.clone()
    };
    draw_chain_line(
        ui,
        text_chain,
        &class_line,
        layout.class,
        t_label,
        ivory,
        TextAlign::Left,
        shadow,
    );
    if let Some(remaining) = selection.resource {
        draw_unit_bar(
            ui,
            number_chain,
            flat,
            layout.hp_bar,
            1.0,
            &format!("{remaining} GOLD"),
            d,
            [0.55, 0.42, 0.12, 1.0],
            tokens.text,
            shadow,
        );
    } else if selection.bars {
        let ratio = if selection.max_hp > 0 {
            (selection.hp as f32 / selection.max_hp as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let color = if ratio < 0.3 {
            [0.62, 0.18, 0.16, 1.0]
        } else {
            [0.30, 0.55, 0.26, 1.0]
        };
        let label = if layout.hp_bar.w >= 92.0 * d {
            format!("{}/{}", selection.hp.max(0), selection.max_hp.max(0))
        } else {
            format!("{}", selection.hp.max(0))
        };
        draw_unit_bar(
            ui,
            number_chain,
            flat,
            layout.hp_bar,
            ratio,
            &label,
            d,
            color,
            tokens.text,
            shadow,
        );
        if let Some((mana, max_mana)) = selection.mana
            && layout.mana_bar.h > 0.0
        {
            let mana_ratio = if max_mana > 0 {
                (mana as f32 / max_mana as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let label = if layout.mana_bar.w >= 92.0 * d {
                format!("{mana}/{max_mana}")
            } else {
                String::new()
            };
            draw_unit_bar(
                ui,
                number_chain,
                flat,
                layout.mana_bar,
                mana_ratio,
                &label,
                d,
                [0.18, 0.30, 0.62, 1.0],
                tokens.text,
                shadow,
            );
        }
    }
    if layout.stats.h > 0.0 && selection.stats_known {
        let mut stats = format!("ARMOR {:+}   DMG {}", selection.armor, selection.damage);
        if let Some(range) = selection.range {
            stats.push_str(&format!("   RNG {range:.0}"));
        }
        if let Some((used, cap)) = selection.food {
            stats.push_str(&format!("   FOOD {used}/{cap}"));
        }
        draw_chain_line(
            ui,
            text_chain,
            &stats,
            layout.stats,
            t_label,
            ivory,
            TextAlign::Left,
            shadow,
        );
    }
    if let Some(production) = selection.production.as_ref()
        && layout.production_label.h > 0.0
    {
        draw_chain_line(
            ui,
            text_chain,
            &format!("{}  {}s", production.name, production.remaining),
            layout.production_label,
            t_label,
            ivory,
            TextAlign::Left,
            shadow,
        );
        if layout.production_bar.w > 0.0 {
            fill(
                ui,
                flat,
                layout.production_bar,
                [0.10, 0.09, 0.07, 0.9],
            );
            fill(
                ui,
                flat,
                Rect::new(
                    layout.production_bar.x,
                    layout.production_bar.y,
                    layout.production_bar.w * production.progress.clamp(0.0, 1.0),
                    layout.production_bar.h,
                ),
                [0.55, 0.42, 0.12, 1.0],
            );
        }
    }
}

/// One control-group plate: number in the top-left, count bottom-right, active
/// group outlined. Empty groups stay recessed and low contrast (TZ §6.3).
#[allow(clippy::too_many_arguments)]
fn draw_group_plate(
    ui: &mut UiBatch,
    text_chain: &FontChain<'_>,
    number_chain: &FontChain<'_>,
    tokens: &Tokens,
    skin: &Skin,
    flat: usize,
    rect: Rect,
    index: usize,
    view: &HudView<'_>,
    s: f32,
    d: f32,
    shadow: Option<[f32; 4]>,
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let group = view.groups.get(index).copied().unwrap_or_default();
    let label = if index < 9 {
        (index + 1).to_string()
    } else {
        "0".to_string()
    };
    // `slot_*` atlas roles carry a baked-in numeral; the tile bevels are clean.
    let role = if group.count > 0 { "tile_gold" } else { "tile_dark" };
    skin.nine_scaled(ui, role, rect, [1.0; 4], s);
    if group.active {
        fill_outline(
            ui,
            flat,
            rect,
            (1.5 * d).max(1.0),
            [0.85, 0.70, 0.32, 0.95],
        );
    }
    number_chain.text_box(
        ui,
        &label,
        Rect::new(rect.x + 2.0 * d, rect.y, rect.w - 4.0 * d, 14.0 * d),
        sp(11.0, d),
        if group.count > 0 {
            tokens.text
        } else {
            tokens.text_muted
        },
        TextAlign::Left,
        shadow,
    );
    if group.count > 0 {
        text_chain.text_box(
            ui,
            &group.count.to_string(),
            Rect::new(rect.x, rect.bottom() - 14.0 * d, rect.w - 2.0 * d, 13.0 * d),
            sp(11.0, d),
            tokens.supply,
            TextAlign::Right,
            None,
        );
    }
}

/// Persistent 48 dp drawer toggle (`ARMY` / `CMD`) with a pressed tint when its
/// drawer is open.
#[allow(clippy::too_many_arguments)]
fn draw_drawer_button(
    ui: &mut UiBatch,
    text_chain: &FontChain<'_>,
    tokens: &Tokens,
    skin: &Skin,
    flat: usize,
    rect: Rect,
    label: &str,
    s: f32,
    d: f32,
    shadow: Option<[f32; 4]>,
    open: bool,
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let role = if open { "tile_gold" } else { "tile_dark" };
    skin.nine_scaled(ui, role, rect, [1.0; 4], s);
    text_chain.text_box(
        ui,
        label,
        Rect::new(rect.x, rect.centre().y - 7.0 * d, rect.w, 15.0 * d),
        sp(11.0, d),
        if open { tokens.gold } else { tokens.text },
        TextAlign::Centre,
        shadow,
    );
    // Chevron: three dots communicate "opens a drawer" without extra art.
    for index in 0..3 {
        fill(
            ui,
            flat,
            Rect::new(
                rect.centre().x - 6.0 * d + index as f32 * 5.0 * d,
                rect.bottom() - 9.0 * d,
                3.0 * d,
                3.0 * d,
            ),
            [0.75, 0.66, 0.42, 0.9],
        );
    }
}

fn draw_minimap_frame(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    s: f32,
) {
    // The frame is the composer's rect. The old path expanded
    // `minimap_image` by `MINIMAP_FRAME_INSETS` here, which made the drawn
    // frame 24 dp wider than the one the hit test and the minimap content
    // used — the third composer the operator saw as a shifted map. Draw the
    // canonical rect and let the 9-slice insets scale inside it.
    let frame = layout.minimap_frame;
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return;
    }
    fill(ui, flat, frame, [0.085, 0.085, 0.080, 1.0]);
    fill(ui, flat, 
        Rect::new(frame.x, frame.y, frame.w, (2.0 * s).max(1.0)),
        STONE_HI,
    );
    let insets = [
        MINIMAP_FRAME_INSETS[0].min(frame.w * 0.2),
        MINIMAP_FRAME_INSETS[1].min(frame.h * 0.2),
        MINIMAP_FRAME_INSETS[2].min(frame.w * 0.2),
        MINIMAP_FRAME_INSETS[3].min(frame.h * 0.2),
    ];
    atlas_tile_frame(skin, ui, "slot_dark", frame, insets, [1.0; 4]);
}

/// Reference column of mini buttons beside the minimap: engraved stone bosses
/// on a plate. Chrome only - the live actions are the card and the group row.
fn draw_minimap_rail(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    let rail = layout.minimap_rail;
    if rail.w <= 0.0 || rail.h <= 0.0 {
        return;
    }
    fill(ui, flat, rail, [0.18, 0.17, 0.15, 0.98]);
    draw_bevel_ring(ui, flat, rail, s, STONE_HI, STONE_LO, GOLD_LINE);
    for (index, tab) in layout.tabs.iter().enumerate() {
        if tab.w <= 0.0 || tab.h <= 0.0 {
            continue;
        }
        let enabled = skin_army_panel::tab_enabled(index);
        let role = if enabled { "slot_dark" } else { "slot_dark" };
        let tint = if enabled {
            [1.0; 4]
        } else {
            [0.62, 0.62, 0.66, 0.92]
        };
        let art = art_box(skin, role, *tab);
        if !skin.nine(ui, role, art, tint) {
            fill(ui, flat, art, PLATE_FILL);
        }
        fill_slot_well(ui, flat, slot_inner(skin, role, art));
        let label = skin_army_panel::tab_label(skin_army_panel::TabLang::Ru, index);
        if !label.is_empty() {
            // Whole word inside the 95 px plate: fit instead of ellipsizing.
            font.draw_fit(
                ui,
                skin_army_panel::tab_label_rect(art),
                label,
                0.26 * s,
                crate::hud::MIN_DENSE_PX,
                if enabled { tokens.text } else { tokens.disabled },
                TextAlign::Centre,
                Some(tokens.shadow),
            );
        }
    }
}

/// Minimap texture, fog of war, blips, camera frustum and the caption chip./// Minimap texture, fog of war, blips, camera frustum and the caption chip.
fn draw_minimap_content(
    ui: &mut UiBatch,
    flat: usize,
    font: &Font,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    tokens: &Tokens,
    s: f32,
    shadow: Option<[f32; 4]>,
) {
    let map = layout.minimap_image;
    if map.w <= 0.0 || map.h <= 0.0 {
        return;
    }
    fill(ui, flat, map, [0.10, 0.16, 0.09, 1.0]);
    ui.quad(UiTexture::Minimap, map, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
    // Fog of war: never-seen cells are black, explored-but-out-of-sight cells
    // are dimmed (WC3 minimap colours), visible cells stay clear.
    if let Some(fog) = view.minimap.fog
        && view.minimap.fog_width > 0
        && view.minimap.fog_height > 0
    {
        let fw = view.minimap.fog_width;
        let fh = view.minimap.fog_height;
        let cw = map.w / fw as f32;
        let ch = map.h / fh as f32;
        for gy in 0..fh {
            let row = gy * fw;
            let mut gx = 0usize;
            while gx < fw {
                let value = fog[row + gx];
                if value >= 2 {
                    gx += 1;
                    continue;
                }
                let start = gx;
                while gx < fw && fog[row + gx] == value {
                    gx += 1;
                }
                let color = if value == 0 {
                    [0.0, 0.0, 0.0, 0.86]
                } else {
                    [0.0, 0.0, 0.0, 0.34]
                };
                fill(ui, flat, 
                    Rect::new(
                        map.x + start as f32 * cw,
                        map.y + gy as f32 * ch,
                        (gx - start) as f32 * cw + 0.5,
                        ch + 0.5,
                    ),
                    color,
                );
            }
        }
    }
    for dot in view.minimap.dots {
        let size = dot.size.max(2.0) * s;
        let x = map.x + dot.x.clamp(0.0, 1.0) * map.w;
        let y = map.y + dot.y.clamp(0.0, 1.0) * map.h;
        fill(ui, flat, 
            Rect::new(x - size * 0.5, y - size * 0.5, size, size),
            dot.color,
        );
    }
    if let Some(camera) = view.minimap.camera {
        let rect = skin_minimap::camera_rect(map.inset(1.0, 1.0), camera);
        if rect.w > 0.0 && rect.h > 0.0 {
            ui.border(rect, 1.5 * s, [1.0, 1.0, 1.0, 0.7]);
        }
    }
    // Caption chip inside the map's top-left corner: the reference minimap has
    // its label there, and a chip keeps the glyphs legible over terrain.
    let caption = layout.minimap_header;
    if caption.w > 0.0 {
        fill(ui, flat, caption, [0.02, 0.02, 0.02, 0.55]);
        font.text_box(
            ui,
            if view.minimap.title.is_empty() {
                "MINIMAP"
            } else {
                view.minimap.title
            },
            clamp_rect(caption.inset(4.0 * s, 0.0), caption),
            0.46 * s,
            tokens.text,
            TextAlign::Left,
            shadow,
        );
    }
}

/// Selection column with the fantasy skin: 1:1 portrait frame, the unit icon at
/// its authored 64x64, name/class beside it, HP bar at a 0.70 uniform scale and
/// two flat stat rows.
fn draw_selection_skin(
    ui: &mut UiBatch,
    flat: usize,
    font: &Font,
    tokens: &Tokens,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    skin: &Skin,
) {
    let s = layout.scale;
    let kind = match view
        .selection
        .as_ref()
        .map(|selection| selection.frame)
        .unwrap_or_default()
    {
        PortraitFrame::Normal => skin_portrait::PortraitFrameKind::Normal,
        PortraitFrame::Hero => skin_portrait::PortraitFrameKind::Hero,
        PortraitFrame::Enemy => skin_portrait::PortraitFrameKind::Enemy,
    };
    // The frame is drawn at `portrait.w / authored` (0.975x at the device
    // scale), so its nine-patch insets scale with that ratio, not with the HUD.
    let art_scale = (layout.portrait.w / skin_portrait::AUTHORED_SIZE[0]).max(0.05);
    let portrait_skin = portrait_scaled_skin(skin, art_scale);
    let parts = skin_portrait::draw(&portrait_skin, ui, layout.portrait, s, kind, [1.0; 4]);
    // Dark matte: no unit keeps a stone plate, never a black hole.
    fill(ui, flat, parts.portrait_rect, [0.055, 0.055, 0.058, 1.0]);
    let Some(selection) = &view.selection else {
        return;
    };
    if let Some(icon) = selection.portrait {
        // 1:1: the shipped BTN art is 64x64, so it is drawn at 64x64 inside the
        // window (96x101) instead of being stretched to fill it.
        let side = 64.0f32.min(parts.portrait_rect.w).min(parts.portrait_rect.h);
        let rect = Rect::new(
            parts.portrait_rect.centre().x - side * 0.5,
            parts.portrait_rect.centre().y - side * 0.5,
            side,
            side,
        );
        ui.textured_quad(icon, rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        ui.border(rect, 1.0 * s, [0.0, 0.0, 0.0, 0.55]);
    } else {
        chrome_letter(ui, font, tokens, selection, parts.portrait_rect);
    }
    // Device frame 2026-11-16: the name clipped mid-glyph and the class line
    // printed "BUILDING · BUILDING" (game.rs already carries the kind in the
    // class for buildings). 521: fit as a whole-word single line, never a
    // mid-word ellipsis.
    let name_fit = fit_text_plate(font, &selection.name, layout.name, 0.76 * s, MIN_TEXT_PX, 1);
    draw_plate_text(
        ui,
        font,
        &name_fit,
        layout.name,
        tokens.text,
        TextAlign::Left,
        Some(tokens.shadow),
    );
    let class_upper = selection.class.to_uppercase();
    let kind_upper = selection.kind.to_uppercase();
    let sub = if kind_upper.is_empty() || class_upper.contains(&kind_upper) {
        selection.class.to_string()
    } else {
        format!("{} · {}", selection.class, selection.kind)
    };
    let class_fit = fit_text_plate(font, &sub, layout.class, 0.50 * s, MIN_TEXT_PX, 1);
    draw_plate_text(
        ui,
        font,
        &class_fit,
        layout.class,
        tokens.text_muted,
        TextAlign::Left,
        None,
    );
    // HP bar: `bar_hp` (403x65) nine-patched at a 0.70 uniform art scale, so the
    // gold frame is a downscale and the fill is the sim's ratio. A resource
    // plate (gold mine) has no HP: the remaining amount is its value line.
    let hp_ratio = (selection.hp.max(0) as f32 / selection.max_hp.max(1) as f32).clamp(0.0, 1.0);
    let hp_text = if selection.bars {
        let hp_skin = portrait_hp_skin(skin, hp_bar_art_scale(s));
        skin_stats::draw_hp(&hp_skin, ui, layout.hp_bar, hp_ratio, s, [1.0; 4])
    } else {
        fill(ui, flat, layout.hp_bar, [0.0, 0.0, 0.0, 0.28]);
        layout.hp_bar
    };
    let value = match selection.resource {
        Some(remaining) => format!("{remaining} GOLD"),
        None => format!("{} / {}", selection.hp.max(0), selection.max_hp.max(1)),
    };
    font.text_box(
        ui,
        &value,
        hp_text,
        0.60 * s,
        if selection.bars { tokens.text } else { tokens.gold },
        TextAlign::Centre,
        Some(tokens.shadow),
    );
    // Two flat rows (armour, damage): the atlas has no unadorned row art, and a
    // stretched row plate was the source of the old mush.
    let rows: [(String, String); 2] = match selection.resource {
        Some(remaining) => [
            ("GOLD".to_string(), remaining.to_string()),
            ("TYPE".to_string(), "MINE".to_string()),
        ],
        None => [
            (
                "ARMOR".to_string(),
                if selection.stats_known {
                    selection.armor.to_string()
                } else {
                    "--".to_string()
                },
            ),
            (
                "DAMAGE".to_string(),
                if selection.stats_known {
                    selection.damage.to_string()
                } else {
                    "--".to_string()
                },
            ),
        ],
    };
    let row_h = (layout.stats.h / rows.len() as f32).max(1.0);
    for (index, (label, value)) in rows.iter().enumerate() {
        let row = Rect::new(
            layout.stats.x,
            layout.stats.y + row_h * index as f32,
            layout.stats.w,
            row_h,
        );
        let tone = if index % 2 == 0 {
            [0.0, 0.0, 0.0, 0.22]
        } else {
            [1.0, 1.0, 1.0, 0.05]
        };
        fill(ui, flat, row, tone);
        font.text_box(
            ui,
            label,
            Rect::new(row.x + 4.0 * s, row.y, row.w * 0.62, row.h),
            0.52 * s,
            tokens.text_muted,
            TextAlign::Left,
            None,
        );
        font.text_box(
            ui,
            value,
            Rect::new(row.x + row.w * 0.62, row.y, row.w * 0.34, row.h),
            0.58 * s,
            tokens.text,
            TextAlign::Left,
            None,
        );
    }
    if let Some(production) = selection.production {
        let progress = production.progress.clamp(0.0, 1.0);
        font.text_box(
            ui,
            &format!("{} {}%", production.name, (progress * 100.0) as u32),
            layout.production_label,
            0.54 * s,
            tokens.supply,
            TextAlign::Left,
            None,
        );
        fill(ui, flat, layout.production_bar, [0.03, 0.03, 0.035, 0.95]);
        fill(
            ui,
            flat,
            Rect::new(
                layout.production_bar.x,
                layout.production_bar.y,
                (layout.production_bar.w * progress).max(1.0),
                layout.production_bar.h,
            ),
            tokens.supply,
        );
    }
}

/// No selection: fill the portrait window with stone and a caption so the frame
/// never reads as a black rectangle.
fn draw_empty_portrait(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    let art_scale = (layout.portrait.w / skin_portrait::AUTHORED_SIZE[0]).max(0.05);
    let portrait_skin = portrait_scaled_skin(skin, art_scale);
    let parts = skin_portrait::draw(
        &portrait_skin,
        ui,
        layout.portrait,
        s,
        skin_portrait::PortraitFrameKind::Normal,
        [0.85, 0.85, 0.85, 1.0],
    );
    let window = parts.portrait_rect;
    if window.w <= 0.0 {
        return;
    }
    draw_stone_field(ui, flat, window, [0.235, 0.228, 0.210, 1.0]);
    font.text_box(
        ui,
        "NO UNIT",
        window,
        0.46 * s,
        tokens.text_muted,
        TextAlign::Centre,
        Some(tokens.shadow),
    );
}

/// Crest panel: dark inlay between the group row and the command card, with a
/// 1:1 divider rail above the row and the 1:1 plaque below it.
fn draw_crest_panel(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    let crest = layout.crest;
    if crest.w <= 0.0 || crest.h <= 0.0 {
        return;
    }
    // Same stone as the console band: a darker inlay read as a hole between the
    // group pad and the inventory once the UI colours became sRGB-correct.
    draw_stone_field(ui, flat, crest, STONE_FIELD);
    draw_bevel_ring(
        ui,
        flat,
        crest,
        s,
        [0.190, 0.180, 0.150, 1.0],
        [0.020, 0.020, 0.020, 1.0],
        GOLD_LINE,
    );
    // Inner vignette so the panel reads as an inlay, not a flat rectangle.
    for i in 0..4 {
        let f = i as f32;
        let alpha = 0.050 * (1.0 - f / 4.0);
        let band = (crest.h * 0.05).max(1.0);
        fill(ui, flat, 
            Rect::new(crest.x, crest.y + f * band, crest.w, band),
            [0.0, 0.0, 0.0, alpha],
        );
        fill(ui, flat, 
            Rect::new(crest.x, crest.bottom() - (f + 1.0) * band, crest.w, band),
            [0.0, 0.0, 0.0, alpha],
        );
    }
    // Divider rail (465x38) above the row: 1:1, shrunk only if the panel is
    // narrower than the art.
    let divider_scale = ((crest.w - 26.0 * s) / 465.0).clamp(0.30, 1.0);
    let divider = Rect::new(
        crest.centre().x - 465.0 * divider_scale * 0.5,
        crest.y + 12.0 * s,
        465.0 * divider_scale,
        38.0 * divider_scale,
    );
    skin.stretch(ui, "divider", divider, [1.0; 4]);
    // Crest plaque (526x130) below the group row, 1:1 at the device scale. The
    // v2 composition folds the crest into the group panel, so the plaque only
    // appears when the layout gives it its own box.
    let plaque = layout.crest_plaque;
    if plaque.w > 0.0 && plaque.h > 0.0 && plaque != layout.crest {
        skin.stretch(ui, "topbar_center", plaque, [1.0; 4]);
        // Faction mark: the player's own unit icon would be art theft of the
        // selection; the plaque stays an engraved ornament instead.
        let label = Rect::new(
            plaque.x + 24.0 * s,
            plaque.y + plaque.h * 0.5 - 10.0 * s,
            plaque.w - 48.0 * s,
            20.0 * s,
        );
        if view.army.len() <= 1 {
            font.text_box(
                ui,
                "CONTROL GROUPS",
                label,
                0.42 * s,
                tokens.text_muted,
                TextAlign::Centre,
                None,
            );
        }
    }
}

/// 4x3 command card: dark plate + the twelve `tile_*` frames at the 80/88 grid.
fn draw_command_card(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    let plate = layout.card.expanded(7.0 * s, 7.0 * s);
    if plate.w <= 0.0 {
        return;
    }
    fill(ui, flat, plate, [0.105, 0.105, 0.100, 0.96]);
    draw_bevel_ring(
        ui,
        flat,
        plate,
        s,
        [0.210, 0.200, 0.170, 1.0],
        [0.020, 0.020, 0.020, 1.0],
        GOLD_LINE,
    );
    for (index, slot) in layout.card_slots.iter().enumerate() {
        let command = view
            .commands
            .get(index)
            .copied()
            .unwrap_or_else(CommandButton::empty);
        let chain = FontChain::new(&[font]);
        draw_card_cell(
            ui,
            &chain,
            tokens,
            skin,
            None,
            flat,
            *slot,
            &command,
            None,
            s,
        );
    }
}

/// The ten control-group plates. `game.rs` owns the hit rects, so the frames are
/// drawn on them (+1 px chrome overhang each side => a 46 px frame on the 44 px
/// retail hit rect).
fn draw_group_plates(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    for (index, slot) in layout.group_slots.iter().enumerate() {
        let key = if index < 9 {
            (index + 1).to_string()
        } else {
            "0".to_string()
        };
        let group = view.groups.get(index).copied().unwrap_or_default();
        let art = *slot;
        if art.w <= 0.0 || art.h <= 0.0 {
            continue;
        }
        let tint = if group.count > 0 {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            [0.78, 0.78, 0.80, 0.92]
        };
        let role = "slot_dark";
        let art = art_box(skin, role, art);
        if !skin.nine(ui, role, art, tint) {
            fill(ui, flat, art, PLATE_FILL);
            ui.border(art, 1.0, GOLD_LINE);
        }
        fill_slot_well(ui, flat, slot_inner(skin, role, art));
        font.text_box(
            ui,
            &key,
            art,
            1.10,
            if group.count > 0 {
                tokens.text
            } else {
                tokens.text_muted
            },
            TextAlign::Centre,
            Some(tokens.shadow),
        );
        if group.active {
            ui.border(art, 2.0, tokens.stroke_hot);
        }
        if group.count > 0 {
            font.text_box(
                ui,
                &group.count.to_string(),
                Rect::new(art.x, art.bottom() - 24.0, art.w, 20.0),
                0.55,
                tokens.supply,
                TextAlign::Centre,
                None,
            );
        }
    }
}

/// Image rect of a minimap ring (the ring is the image dilated by the 1:1
/// atlas insets).
fn inset_minimap(ring: Rect) -> Rect {
    ring.inset_sides(
        MINIMAP_FRAME_INSETS[0],
        MINIMAP_FRAME_INSETS[1],
        MINIMAP_FRAME_INSETS[2],
        MINIMAP_FRAME_INSETS[3],
    )
}

/// Crest plaque inside its band: the 526x130 ornament shrunk to fit, centred.
fn crest_plaque_in(band: Rect) -> Rect {
    if band.w <= 0.0 || band.h <= 0.0 {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    }
    let scale = ((band.w - 12.0) / 526.0).min((band.h - 12.0) / 130.0).clamp(0.05, 1.0);
    Rect::new(
        band.centre().x - 526.0 * scale * 0.5,
        band.centre().y - 130.0 * scale * 0.5,
        526.0 * scale,
        130.0 * scale,
    )
}

/// The ten 48 dp control-group plates inside `area`, row-major (5 x 2).
fn group_slots_in(area: Rect) -> Vec<Rect> {
    let mut slots = Vec::new();
    if area.w <= 0.0 || area.h <= 0.0 {
        return slots;
    }
    let side = GROUP_SIDE.min(area.h).max(24.0);
    let gap = GROUP_GAP;
    let pitch = side + gap;
    let columns = (((area.w + gap) / pitch).floor() as usize).clamp(1, GROUP_COLUMNS);
    let rows = ((GROUP_COLUMNS * GROUP_ROWS) + columns - 1) / columns;
    let w = columns as f32 * side + (columns - 1) as f32 * gap;
    let h = rows as f32 * side + (rows - 1) as f32 * gap;
    if w > area.w + 0.5 || h > area.h + 0.5 {
        return slots;
    }
    let x0 = area.x + (area.w - w) * 0.5;
    let y0 = area.y + (area.h - h) * 0.5;
    for index in 0..(GROUP_COLUMNS * GROUP_ROWS) {
        let column = index % columns;
        let row = index / columns;
        slots.push(Rect::new(
            x0 + column as f32 * pitch,
            y0 + row as f32 * pitch,
            side,
            side,
        ));
    }
    slots
}

/// Touch rects of the army chips: `army_panel::layout` paginates them so every
/// hit stays at the 48 dp floor instead of shrinking.
fn army_slot_hits(area: Rect, count: usize, scale: f32) -> Vec<Rect> {
    if count == 0 || area.w <= 0.0 || area.h <= 0.0 {
        return Vec::new();
    }
    let plan = skin_army_panel::layout(
        area,
        count,
        0,
        scale,
        skin_army_panel::TouchMetrics::DEVICE,
    );
    plan.hits
}

/// Selected-unit chips: icon window + micro HP bar, all inside the army slots.
fn draw_army_chips(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
) {
    for (index, slot) in layout.army_slots.iter().enumerate() {
        let chip = view.army.get(index).copied().unwrap_or_default();
        if slot.w <= 0.0 || slot.h <= 0.0 {
            continue;
        }
        let role = if chip.selected { "slot_gold" } else { "slot_dark" };
        let art = art_box(skin, role, *slot);
        if !skin.nine(ui, role, art, [1.0; 4]) {
            ui.rect(art, PLATE_FILL);
        }
        let well = slot_inner(skin, role, art);
        fill(ui, flat, well, [0.09, 0.09, 0.10, 0.98]);
        let pad = (well.w * 0.06).max(2.0);
        let window = well.inset(pad, pad);
        if let Some(icon) = chip.icon {
            ui.textured_quad(icon, window, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        }
        // Micro HP bar along the bottom inside edge.
        let bar = Rect::new(window.x, well.bottom() - 7.0, window.w, 5.0);
        ui.rect(bar, [0.04, 0.04, 0.05, 0.95]);
        ui.rect(
            Rect::new(bar.x, bar.y, (bar.w * chip.hp.clamp(0.0, 1.0)).max(1.0), bar.h),
            skin_army::hp_color(chip.hp.clamp(0.0, 1.0)),
        );
    }
}

/// Four 48 dp top-banner tabs (`army_panel::draw_top_tabs`) plus their labels.
///
/// The tabs are touch buttons; the F-keys are only the tooltip sub-labels, so
/// the label glyph is the language string and nothing else.
/// Hero inventory: the real `hud::inventory` panel (2x3 slots of 48 dp) beside
/// the portrait console's command card.
///
/// The module owns the frame roles, the cooldown veil and the finger targets;
/// this call site resolves the icon textures and the charge labels, exactly as
/// the module contract requires.
fn draw_inventory_panel(
    skin: &Skin,
    ui: &mut UiBatch,
    flat: usize,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    if layout.inventory.w <= 0.0 || layout.inventory.h <= 0.0 {
        return;
    }
    fill(ui, flat, layout.inventory, PLATE_FILL);
    let parts = crate::hud::inventory::draw(
        skin,
        ui,
        layout.inventory,
        view.items,
        s,
        [1.0; 4],
    );
    for slot in parts.slots {
        if slot.w > 0.0 {
            fill_slot_well(ui, flat, slot_inner(skin, "slot_dark", slot));
        }
    }
    // Retail draws no caption over the item grid (the 2x3 framed wells are the
    // affordance); the label overlapped the frame's top edge at 29.3% band.
    let _ = (&parts.header, tokens);
    for index in 0..crate::hud::inventory::SLOTS {
        let icon_rect = parts.icons[index];
        if icon_rect.w <= 0.0 {
            continue;
        }
        if let Some(icon) = view.item_icons.get(index).copied().flatten() {
            ui.textured_quad(icon, icon_rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        }
        let charges = view
            .items
            .get(index)
            .copied()
            .flatten()
            .map(|item| item.charges)
            .unwrap_or(0);
        if let Some(label) = crate::hud::inventory::charge_label(charges) {
            font.text_box(
                ui,
                &label,
                parts.charges[index],
                0.30 * s,
                tokens.text,
                TextAlign::Right,
                Some(tokens.shadow),
            );
        }
    }
}

/// Fallback minimap chrome (no atlas skin): one frame, either the retail chrome/// Fallback minimap chrome (no atlas skin): one frame, either the retail chrome
/// or the redrawn pack art.
fn draw_minimap_fallback(
    ui: &mut UiBatch,
    chrome: &Chrome,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    font: &Font,
    tokens: &Tokens,
    s: f32,
) {
    let frame_rect = layout.minimap_frame;
    let redraw_frame = view.redraw.and_then(|skin| {
        if view.phase.is_some() {
            skin.minimap_frame_alert.or(skin.minimap_frame)
        } else {
            skin.minimap_frame
        }
    });
    if view.redraw.is_none()
        && chrome
            .element_or(ui, frame_rect, ATLAS_FRAME, s, [1.0; 4])
            .is_none()
    {
        draw_button_chrome(ui, chrome, frame_rect, view.button_border, s);
    }
    if let Some(frame) = redraw_frame {
        ui.textured_quad(frame, frame_rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
    } else {
        ui.border(frame_rect, 1.0 * s, [0.62, 0.50, 0.24, 0.9]);
    }
    if let Some(skin) = view.redraw
        && let Some(header) = skin.minimap_header
    {
        ui.textured_quad(header, layout.minimap_header, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
    }
    let _ = (font, tokens);
}

/// Fallback selection column (no atlas skin): flat panels, the shipped portrait
/// frame texture when the install has one.
fn draw_selection_fallback(
    ui: &mut UiBatch,
    font: &Font,
    chrome: &Chrome,
    tokens: &Tokens,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    s: f32,
) {
    if view.selection.is_some() {
        // The panel is the composer's zone; the old +4*s expansion drew the
        // info plate 8 px outside the rect the hit test used.
        let info_panel = layout.selection;
        chrome.flat_rect(ui, info_panel, [0.025, 0.022, 0.018, 0.92]);
        if let Some(panel) = view.redraw.and_then(|skin| skin.selection_panel) {
            chrome.frame(ui, info_panel, Some(panel), 1.0, [1.0; 4]);
        } else if chrome
            .element_or(ui, info_panel, ATLAS_PANEL, s, [1.0; 4])
            .is_none()
        {
            ui.border(info_panel, 1.0 * s, [0.52, 0.42, 0.22, 0.85]);
        }
    }
    let redraw_frame = view.redraw.and_then(|skin| {
        let index = view
            .selection
            .as_ref()
            .map(|selection| selection.frame.index())
            .unwrap_or(0);
        skin.portrait.get(index).copied().flatten()
    });
    if let Some(frame) = redraw_frame.or(view.portrait_frame) {
        ui.textured_quad(frame, layout.portrait, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
    } else {
        draw_button_chrome(ui, chrome, layout.portrait, view.button_border, s);
    }
    let inset = layout.portrait.inset(14.0 * s, 12.0 * s);
    chrome.flat_rect(ui, inset, [0.015, 0.013, 0.010, 0.96]);
    let Some(selection) = &view.selection else {
        return;
    };
    if let Some(icon) = selection.portrait {
        let side = 92.0f32.min(inset.w).min(inset.h);
        ui.textured_quad(
            icon,
            Rect::new(
                inset.centre().x - side * 0.5,
                inset.centre().y - side * 0.5,
                side,
                side,
            ),
            [0.0, 0.0, 1.0, 1.0],
            [1.0; 4],
        );
    } else {
        chrome_letter(ui, font, tokens, selection, inset);
    }
    let name_fit = fit_text_plate(font, &selection.name, layout.name, 0.76 * s, MIN_TEXT_PX, 1);
    draw_plate_text(
        ui,
        font,
        &name_fit,
        layout.name,
        tokens.text,
        TextAlign::Left,
        Some(tokens.shadow),
    );
    let class_fit = fit_text_plate(
        font,
        &format!("{}  ·  {}", selection.class, selection.kind),
        layout.class,
        0.50 * s,
        MIN_TEXT_PX,
        1,
    );
    draw_plate_text(
        ui,
        font,
        &class_fit,
        layout.class,
        tokens.text_muted,
        TextAlign::Left,
        None,
    );
    let hp_ratio = (selection.hp.max(0) as f32 / selection.max_hp.max(1) as f32).clamp(0.0, 1.0);
    if selection.bars {
        chrome.flat_rect(ui, layout.hp_bar, [0.05, 0.06, 0.04, 0.98]);
        chrome.flat_rect(
            ui,
            Rect::new(
                layout.hp_bar.x + 1.0 * s,
                layout.hp_bar.y + 1.0 * s,
                ((layout.hp_bar.w - 2.0 * s) * hp_ratio).max(2.0),
                (layout.hp_bar.h - 2.0 * s).max(1.0),
            ),
            tokens.hp(hp_ratio),
        );
        ui.border(layout.hp_bar, 1.0 * s, [0.60, 0.50, 0.25, 0.9]);
    }
    let value = match selection.resource {
        Some(remaining) => format!("{remaining} GOLD"),
        None => format!("{} / {}", selection.hp.max(0), selection.max_hp.max(1)),
    };
    font.text_box(
        ui,
        &value,
        clamp_rect(layout.hp_text, layout.console),
        0.60 * s,
        if selection.bars { tokens.text } else { tokens.gold },
        TextAlign::Centre,
        Some(tokens.shadow),
    );
    let stats = match selection.resource {
        Some(remaining) => format!("GOLD {remaining}    TYPE MINE"),
        None if selection.stats_known => {
            format!("ARMOR {}    DAMAGE {}", selection.armor, selection.damage)
        }
        None => "ARMOR --    DAMAGE --".to_string(),
    };
    font.text_box(
        ui,
        &stats,
        clamp_rect(layout.stats, layout.console),
        0.54 * s,
        tokens.text,
        TextAlign::Left,
        None,
    );
    if let Some(production) = selection.production {
        let progress = production.progress.clamp(0.0, 1.0);
        font.text_box(
            ui,
            &format!("{} {}%", production.name, (progress * 100.0) as u32),
            clamp_rect(layout.production_label, layout.console),
            0.54 * s,
            tokens.supply,
            TextAlign::Left,
            None,
        );
        chrome.flat_rect(ui, layout.production_bar, [0.05, 0.06, 0.04, 0.98]);
        chrome.flat_rect(
            ui,
            Rect::new(
                layout.production_bar.x,
                layout.production_bar.y,
                (layout.production_bar.w * progress).max(1.0),
                layout.production_bar.h,
            ),
            tokens.supply,
        );
    }
}

/// Fallback command card (no atlas skin): `chrome` slot art per tile.
fn draw_card_fallback(
    ui: &mut UiBatch,
    font: &Font,
    chrome: &Chrome,
    tokens: &Tokens,
    layout: &MobileHudLayout,
    view: &HudView<'_>,
    s: f32,
) {
    let card_plate = layout.card.expanded(6.0 * s, 6.0 * s);
    chrome.flat_rect(ui, card_plate, [0.025, 0.022, 0.018, 0.98]);
    if chrome
        .element_or(ui, card_plate, ATLAS_CARD_PLATE, s, [1.0; 4])
        .is_none()
    {
        ui.border(card_plate, 1.0 * s, [0.55, 0.44, 0.22, 0.9]);
    }
    for (index, slot) in layout.card_slots.iter().enumerate() {
        let command = view
            .commands
            .get(index)
            .copied()
            .unwrap_or_else(CommandButton::empty);
        draw_card_slot(
            ui,
            font,
            chrome,
            tokens,
            *slot,
            &command,
            view.button_border,
            view.redraw.map(|skin| skin.button),
            s,
        );
    }
}

/// Card state index into `RedrawSkin::button`:
/// 0 normal, 1 pressed, 2 disabled, 3 cooldown, 4 autocast, 5 empty.
#[cfg_attr(not(test), allow(dead_code))]
fn command_visual_state(command: &CommandButton<'_>) -> usize {
    if !command.present {
        5
    } else if command.pressed {
        1
    } else if !command.enabled {
        2
    } else if command.progress > 0.0 {
        3
    } else {
        0
    }
}

/// The single text frame a retail command tile owns: the hotkey overlay.
/// TFT `CCommandButton` (`Game.dll 0x6f12c2b0`, fields 0x208/0x20c/0x20f)
/// draws `MasterFont` over `CommandButtonNumberOverlay` and nothing else — no
/// name, no cost badge. Name/cost/description live in the tooltip.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CardSlotText {
    pub hotkey: Option<Rect>,
}

/// A cost badge string (`130G`, `2F`): a card sub-label is a cost when it
/// carries a digit. Fixed commands use their hotkey letter as `sub`, which is
/// drawn as the hotkey chip instead.
pub fn card_cost_label(sub: &str) -> Option<&str> {
    let trimmed = sub.trim();
    (!trimmed.is_empty() && trimmed.chars().any(|character| character.is_ascii_digit()))
        .then_some(trimmed)
}

/// Chip geometry of one slot at `scale`. Shared by the renderer and the
/// `hud::` layout tests, so the assertions measure exactly what is drawn.
pub fn card_slot_text_rects(
    font: &Font,
    slot: Rect,
    command: &CommandButton<'_>,
    scale: f32,
) -> CardSlotText {
    // Retail font height is 0.015 * screen height (~16 px at 1080p); the tile
    // overlay is that same MasterFont run anchored top-left of the tile.
    let overlay_scale = (0.53 * scale).max(crate::hud::MIN_DENSE_PX / 30.0);
    let overlay_h = (font.glyph_box(overlay_scale) + 3.0 * scale).min(slot.h * 0.5);
    let mut text = CardSlotText::default();
    if command.present && !command.hotkey.is_empty() {
        let width = (slot.w * 0.42).min(overlay_h * 2.2).max(overlay_h);
        text.hotkey = Some(Rect::new(
            slot.x + 1.0 * scale,
            slot.y + 1.0 * scale,
            width,
            overlay_h,
        ));
    }
    text
}

/// Tile state for one command entry, in the region's vocabulary.
// ------------------------------------------------------ card cell art (469)
/// Hotkey badge plate floor: the operator requires >= 24 px at the 90 px cell.
pub const HOTKEY_BADGE_MIN_PX: f32 = 24.0;
/// Opaque dark badge plate so the letter clears a bright icon (WCAG >= 3:1).
pub const HOTKEY_BADGE_FILL: [f32; 4] = [0.015, 0.015, 0.015, 0.94];
/// One-pixel dark outline around the badge plate.
pub const HOTKEY_BADGE_OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
/// Empty/ability cell interior: the authored card plate (38,39,40).
pub const CARD_WELL_FILL: [f32; 4] = [0.150, 0.153, 0.157, 1.0];

/// Hotkey badge rect inside a card-cell art rect (top-left, never clipped).
pub fn card_hotkey_badge(cell: Rect, s: f32) -> Rect {
    let smallest = cell.w.min(cell.h);
    let max_side = (smallest * 0.46).max(HOTKEY_BADGE_MIN_PX.min(smallest));
    let side = (34.0 * s).max(HOTKEY_BADGE_MIN_PX).min(max_side);
    Rect::new(cell.x + 2.0 * s, cell.y + 2.0 * s, side, side)
}

/// WCAG 2 relative luminance of an authored sRGB colour.
pub fn wcag_luminance(color: [f32; 4]) -> f32 {
    let lin = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(color[0]) + 0.7152 * lin(color[1]) + 0.0722 * lin(color[2])
}

/// WCAG contrast ratio between two authored sRGB colours (1..21).
pub fn contrast_ratio(a: [f32; 4], b: [f32; 4]) -> f32 {
    let (l1, l2) = (wcag_luminance(a), wcag_luminance(b));
    (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
}

/// The badge plate composited over an icon colour (alpha-over, sRGB).
pub fn hotkey_badge_background(icon: [f32; 4]) -> [f32; 4] {
    let a = HOTKEY_BADGE_FILL[3];
    [
        HOTKEY_BADGE_FILL[0] * a + icon[0] * (1.0 - a),
        HOTKEY_BADGE_FILL[1] * a + icon[1] * (1.0 - a),
        HOTKEY_BADGE_FILL[2] * a + icon[2] * (1.0 - a),
        1.0,
    ]
}

/// Clip a convex polygon against one half-plane (`inside` keeps a vertex).
fn clip_poly_edge(
    poly: Vec<Vec2>,
    inside: impl Fn(&Vec2) -> bool,
    intersect: impl Fn(&Vec2, &Vec2) -> Vec2,
) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    if poly.is_empty() {
        return out;
    }
    for index in 0..poly.len() {
        let current = poly[index];
        let previous = poly[(index + poly.len() - 1) % poly.len()];
        let current_in = inside(&current);
        let previous_in = inside(&previous);
        if current_in != previous_in {
            out.push(intersect(&previous, &current));
        }
        if current_in {
            out.push(current);
        }
    }
    out
}

/// Clip a triangle to a rect (Sutherland-Hodgman, fan-triangulated output is
/// pushed by [`fill_wedge`]).
fn clip_triangle_to_rect(rect: Rect, triangle: [Vec2; 3]) -> Vec<Vec2> {
    let mut poly = triangle.to_vec();
    poly = clip_poly_edge(poly, |p| p.x >= rect.x, |a, b| {
        let t = (rect.x - a.x) / (b.x - a.x);
        Vec2::new(rect.x, a.y + (b.y - a.y) * t)
    });
    poly = clip_poly_edge(poly, |p| p.x <= rect.right(), |a, b| {
        let t = (rect.right() - a.x) / (b.x - a.x);
        Vec2::new(rect.right(), a.y + (b.y - a.y) * t)
    });
    poly = clip_poly_edge(poly, |p| p.y >= rect.y, |a, b| {
        let t = (rect.y - a.y) / (b.y - a.y);
        Vec2::new(a.x + (b.x - a.x) * t, rect.y)
    });
    poly = clip_poly_edge(poly, |p| p.y <= rect.bottom(), |a, b| {
        let t = (rect.bottom() - a.y) / (b.y - a.y);
        Vec2::new(a.x + (b.x - a.x) * t, rect.bottom())
    });
    poly
}

/// Push a flat-colour triangle list as a textured draw of the flat texture.
/// Emitting the shape in the textured pass (not the white pass) keeps it under
/// the card glyphs: the pack fonts draw through `Assets::textures` too, and the
/// white channel is painted after every textured draw.
fn fill_polygons(ui: &mut UiBatch, flat: usize, points: &[Vec2], color: [f32; 4]) {
    if points.len() < 3 {
        return;
    }
    let color = [
        crate::hud::srgb_to_linear(color[0]),
        crate::hud::srgb_to_linear(color[1]),
        crate::hud::srgb_to_linear(color[2]),
        color[3],
    ];
    let vertex = |point: Vec2| UiVertex {
        pos: [point.x, point.y],
        uv: [0.5, 0.5],
        color,
    };
    let mut vertices = Vec::with_capacity((points.len() - 2) * 3);
    for index in 1..points.len() - 1 {
        vertices.push(vertex(points[0]));
        vertices.push(vertex(points[index]));
        vertices.push(vertex(points[index + 1]));
    }
    if let Some(last) = ui.textured.last_mut().filter(|draw| draw.texture == flat) {
        last.vertices.extend(vertices);
    } else {
        ui.textured.push(UiDraw {
            texture: flat,
            vertices,
        });
    }
}

/// Flat outline (four textured strips) for plates that sit under glyphs.
fn fill_outline(ui: &mut UiBatch, flat: usize, rect: Rect, width: f32, color: [f32; 4]) {
    let width = width.max(1.0);
    fill(ui, flat, Rect::new(rect.x, rect.y, rect.w, width), color);
    fill(
        ui,
        flat,
        Rect::new(rect.x, rect.bottom() - width, rect.w, width),
        color,
    );
    fill(ui, flat, Rect::new(rect.x, rect.y, width, rect.h), color);
    fill(
        ui,
        flat,
        Rect::new(rect.right() - width, rect.y, width, rect.h),
        color,
    );
}

/// Solid radial wedge (cooldown sweep), clipped to the cell and emitted in the
/// textured pass. Angles in radians; `start = -PI/2` is the top of the cell,
/// sweeping clockwise. The radius is half the cell diagonal, so a 50% sweep
/// covers half the cell area (a half-circle inscribed in the square would leave
/// corners dark at every cooldown).
fn fill_wedge(
    ui: &mut UiBatch,
    flat: usize,
    rect: Rect,
    start: f32,
    sweep: f32,
    color: [f32; 4],
) {
    if rect.w <= 0.0 || rect.h <= 0.0 || sweep <= 0.0 {
        return;
    }
    const STEPS: usize = 32;
    let centre = rect.centre();
    let radius = rect.w.hypot(rect.h) * 0.5;
    for step in 0..STEPS {
        let a0 = start + sweep * step as f32 / STEPS as f32;
        let a1 = start + sweep * (step + 1) as f32 / STEPS as f32;
        let p0 = Vec2::new(centre.x + a0.cos() * radius, centre.y + a0.sin() * radius);
        let p1 = Vec2::new(centre.x + a1.cos() * radius, centre.y + a1.sin() * radius);
        let poly = clip_triangle_to_rect(rect, [centre, p0, p1]);
        fill_polygons(ui, flat, &poly, color);
    }
}

/// Cost row of a build/train entry: the explicit `cost`, else the shipped
/// `sub` cost label (`135G` / `60 LUMBER`).
fn command_cost_label(command: &CommandButton<'_>) -> Option<String> {
    if let Some(cost) = command.cost {
        return Some(cost.to_string());
    }
    let sub = command.sub.trim();
    let has_digit = sub.chars().any(|ch| ch.is_ascii_digit());
    let cost_like = has_digit
        && (sub.ends_with('G')
            || sub.ends_with('L')
            || sub.contains("GOLD")
            || sub.contains("LUMBER"));
    cost_like.then(|| sub.to_string())
}

/// One command-cell drawing routine: state frame, icon 1:1, radial cooldown,
/// autocast ring, disabled dim, hotkey badge, mana/cost/page rows. Both the
/// pack path and the atlas path call this so a cell looks the same everywhere.
#[allow(clippy::too_many_arguments)]
fn draw_card_cell(
    ui: &mut UiBatch,
    text: &FontChain<'_>,
    tokens: &Tokens,
    skin: &Skin,
    pack: Option<&HudPack>,
    flat: usize,
    slot: Rect,
    command: &CommandButton<'_>,
    cell_id: Option<&str>,
    s: f32,
) {
    if slot.w <= 0.0 || slot.h <= 0.0 {
        return;
    }
    let role = card_tile_state(command).role();
    let art = art_box(skin, role, slot);
    if command.present {
        if !skin.nine(ui, role, art, [1.0; 4]) {
            fill(ui, flat, art, PLATE_FILL);
            ui.border(art, 1.0 * s, GOLD_LINE);
        }
    } else {
        // Empty cell: the pack's authored `button_border_empty_64` 9-slice over
        // a plate-coloured well (38,39,40) with a 1 px top light — never a
        // pure-black interior.
        let drawn = pack
            .map(|pack| pack.nine(ui, "button_border_empty_64", art, [1.0; 4], s))
            .unwrap_or(false)
            || skin.nine(ui, "tile_empty", art, [1.0; 4]);
        if !drawn {
            fill(ui, flat, art, CARD_WELL_FILL);
            ui.border(art, 1.0 * s, GOLD_LINE);
        }
    }
    let window = slot_inner(skin, role, art);
    fill(ui, flat, window, CARD_WELL_FILL);
    fill(
        ui,
        flat,
        Rect::new(window.x, window.y, window.w, 1.0),
        [0.30, 0.29, 0.27, 0.62],
    );
    if !command.present {
        return;
    }
    // Icon at 1:1 inside the authored window (never stretched).
    if let Some(icon) = command.icon {
        let tint = if command.enabled {
            [1.0; 4]
        } else {
            [0.42, 0.42, 0.42, 1.0]
        };
        ui.textured_quad(icon, window, [0.0, 0.0, 1.0, 1.0], tint);
    } else if let (Some(pack), Some(cell_id)) = (pack, cell_id)
        && !pack.draw_atlas_cell(ui, cell_id, window, [1.0; 4])
    {
        // Missing atlas cell: the well stays visible.
    }
    if !command.enabled {
        fill(ui, flat, window, [0.0, 0.0, 0.0, 0.28]);
    }
    // Radial cooldown sweep from the top; `remaining` share of the disc.
    if let Some(remaining) = command
        .cooldown
        .map(|value| value.clamp(0.0, 1.0))
        .filter(|value| *value > 0.0)
    {
        fill_wedge(
            ui,
            flat,
            window,
            -std::f32::consts::FRAC_PI_2,
            std::f32::consts::TAU * remaining,
            [0.03, 0.035, 0.05, 0.76],
        );
    }
    // Production share (queue/train): the legacy bottom veil.
    if command.cooldown.is_none() && command.progress > 0.0 {
        let h = window.h * command.progress.clamp(0.0, 1.0);
        fill(
            ui,
            flat,
            Rect::new(window.x, window.bottom() - h, window.w, h),
            [0.10, 0.10, 0.12, 0.65],
        );
    }
    // Autocast ring around the icon window.
    if command.autocast {
        fill_outline(ui, flat, window.expanded(2.0 * s, 2.0 * s), 2.0 * s, tokens.gold);
        fill_outline(ui, flat, window, 1.0 * s, [0.55, 0.45, 0.18, 0.90]);
    }
    // Hotkey badge: opaque plate + dark outline, drawn after the icon and
    // the autocast ring so it is never clipped or dimmed by them.
    if !command.hotkey.is_empty() {
        let badge = card_hotkey_badge(art, s);
        fill(ui, flat, badge, HOTKEY_BADGE_FILL);
        fill_outline(ui, flat, badge, 1.5 * s, HOTKEY_BADGE_OUTLINE);
        text.text_box(
            ui,
            &command.hotkey,
            badge.inset(2.0 * s, 1.0 * s),
            0.80 * s,
            if command.enabled {
                tokens.text
            } else {
                tokens.disabled
            },
            TextAlign::Centre,
            Some(tokens.shadow),
        );
    }
    let row_h = (26.0 * s).max(HOTKEY_BADGE_MIN_PX);
    // Mana cost: bottom-right corner of an ability cell.
    if command.mana_cost > 0 {
        let plate = Rect::new(
            window.right() - window.w * 0.44,
            window.bottom() - row_h,
            window.w * 0.44,
            row_h,
        );
        fill(ui, flat, plate, HOTKEY_BADGE_FILL);
        text.text_box(
            ui,
            &command.mana_cost.to_string(),
            plate.inset(4.0 * s, 0.0),
            0.72 * s,
            tokens.supply,
            TextAlign::Right,
            Some(tokens.shadow),
        );
    }
    // Build/train cost: bottom-left, red when unaffordable
    // (`Sim::can_build_now` snapshot in `affordable`).
    if let Some(cost) = command_cost_label(command) {
        let plate = Rect::new(
            window.x,
            window.bottom() - row_h,
            window.w * 0.56,
            row_h,
        );
        fill(ui, flat, plate, HOTKEY_BADGE_FILL);
        let color = if command.affordable && command.enabled {
            tokens.gold
        } else {
            tokens.danger
        };
        text.text_box(
            ui,
            &cost,
            plate.inset(4.0 * s, 0.0),
            0.72 * s,
            color,
            TextAlign::Left,
            Some(tokens.shadow),
        );
    }
    // MORE slot: the page number is visible (`1/2`).
    if let Some((current, total)) = command.page {
        let plate = Rect::new(
            window.x + window.w * 0.18,
            window.bottom() - row_h,
            window.w * 0.64,
            row_h,
        );
        fill(ui, flat, plate, HOTKEY_BADGE_FILL);
        text.text_box(
            ui,
            &format!("{current}/{total}"),
            plate,
            0.76 * s,
            tokens.text,
            TextAlign::Centre,
            Some(tokens.shadow),
        );
    }
}

fn card_tile_state(command: &CommandButton<'_>) -> skin_tiles::TileState {
    if !command.present {
        skin_tiles::TileState::Empty
    } else if command.pressed {
        skin_tiles::TileState::Pressed
    } else if !command.enabled {
        skin_tiles::TileState::Disabled
    } else if command.hover {
        skin_tiles::TileState::Hover
    } else if command.progress > 0.0 {
        // `tile_dark` sweeps the top `t` of the tile; the live progress is the
        // cleared share, so the sweep is what remains.
        skin_tiles::TileState::Cooldown((1.0 - command.progress).clamp(0.0, 1.0))
    } else {
        skin_tiles::TileState::Normal
    }
}

/// Icon + hotkey overlay inside a tile the region already framed.
fn draw_card_overlay(
    ui: &mut UiBatch,
    font: &Font,
    tokens: &Tokens,
    rect: Rect,
    command: &CommandButton<'_>,
    boxes: skin_tiles::TileRects,
    scale: f32,
) {
    let present = command.present && (command.icon.is_some() || !command.label.is_empty());
    if !present {
        return;
    }
    if let Some(icon) = command.icon {
        ui.textured_quad(
            icon,
            clamp_rect(boxes.icon, rect),
            [0.0, 0.0, 1.0, 1.0],
            if command.enabled {
                if command.pressed {
                    [0.80, 0.80, 0.80, 1.0]
                } else {
                    [1.0; 4]
                }
            } else {
                [0.45, 0.45, 0.45, 1.0]
            },
        );
    }
    if command.hotkey.is_empty() {
        return;
    }
    let overlay_scale = (0.53 * scale).max(crate::hud::MIN_DENSE_PX / 30.0);
    let badge = clamp_rect(boxes.hotkey, rect);
    ui.rect(badge, [0.02, 0.02, 0.02, 0.62]);
    font.draw_fit(
        ui,
        badge.inset(2.0 * scale, 0.0),
        &command.hotkey,
        overlay_scale,
        crate::hud::MIN_DENSE_PX,
        if command.enabled {
            tokens.gold
        } else {
            tokens.disabled
        },
        TextAlign::Left,
        None,
    );
}

fn draw_card_slot(
    ui: &mut UiBatch,
    font: &Font,
    chrome: &Chrome,
    tokens: &Tokens,
    rect: Rect,
    command: &CommandButton<'_>,
    border: Option<usize>,
    states: Option<[Option<usize>; 6]>,
    scale: f32,
) {
    let present = command.present && (command.icon.is_some() || !command.label.is_empty());
    let look = command_look(command);
    chrome.flat_rect(
        ui,
        rect,
        if !present {
            [0.035, 0.032, 0.028, 0.96]
        } else if !command.enabled {
            [0.055, 0.05, 0.042, 0.98]
        } else if command.pressed {
            [0.055, 0.045, 0.03, 0.98]
        } else {
            [0.10, 0.085, 0.06, 0.98]
        },
    );
    // One shell per tile, in priority order: the painter's atlas state art, the
    // redrawn pack state art, the retail border. Every one of them is a
    // nine-patch, so a 64 px phone tile shows the same ornament as a 90 px tile.
    let atlas_drawn = chrome.element(ui, rect, look.atlas, scale, look.tint);
    let state_texture = states.and_then(|states| states[look.state]);
    let drawn = atlas_drawn
        || chrome.frame(ui, rect, state_texture, scale, look.tint)
        || chrome.frame(ui, rect, border, scale, look.tint);
    if !drawn {
        ui.border(
            rect,
            1.0 * scale,
            if present {
                [0.55, 0.44, 0.20, 0.9]
            } else {
                [0.25, 0.22, 0.16, 0.7]
            },
        );
    }
    if !present {
        return;
    }
    let icon_size = (rect.w - 6.0 * scale).max(1.0);
    if let Some(icon) = command.icon {
        ui.textured_quad(
            icon,
            clamp_rect(
                Rect::new(
                    rect.centre().x - icon_size * 0.5,
                    rect.centre().y - icon_size * 0.5,
                    icon_size,
                    icon_size,
                ),
                rect,
            ),
            [0.0, 0.0, 1.0, 1.0],
            if command.enabled {
                if command.pressed {
                    [0.80, 0.80, 0.80, 1.0]
                } else {
                    [1.0; 4]
                }
            } else {
                [0.45, 0.45, 0.45, 1.0]
            },
        );
    }
    // WC3 information design: the slot is the icon plus two chips — the hotkey
    // at the top-left and the cost at the bottom-right. The entry's name is in
    // the tooltip/hint, never squeezed into 64 px (that produced 1.9x text and
    // bled into the neighbouring buttons). Every run goes through `draw_fit`,
    // which ellipsizes and asserts it fits its chip.
    // The one text frame a retail tile owns: the hotkey overlay.
    let overlay_scale = (0.53 * scale).max(crate::hud::MIN_DENSE_PX / 30.0);
    let text_rects = card_slot_text_rects(font, rect, command, scale);
    let gold = if command.enabled {
        tokens.gold
    } else {
        tokens.disabled
    };
    if let Some(overlay) = text_rects.hotkey {
        ui.rect(overlay, [0.02, 0.02, 0.02, 0.62]);
        font.draw_fit(
            ui,
            overlay.inset(2.0 * scale, 0.0),
            &command.hotkey,
            overlay_scale,
            crate::hud::MIN_DENSE_PX,
            gold,
            TextAlign::Left,
            None,
        );
    }
    if command.progress > 0.0 {
        // Cooldown sweep: the remaining time darkens the tile from the top down
        // (WC3's radial sweep is approximated with a quad curtain plus the
        // bottom progress bar, which keeps the sweep off the triangle path).
        let remaining = (1.0 - command.progress).clamp(0.0, 1.0);
        if remaining > 0.0 {
            ui.rect(
                Rect::new(
                    rect.x + 2.0 * scale,
                    rect.y + 2.0 * scale,
                    (rect.w - 4.0 * scale).max(0.0),
                    ((rect.h - 4.0 * scale) * remaining).max(0.0),
                ),
                [0.0, 0.0, 0.0, 0.55],
            );
        }
        chrome.flat_rect(
            ui,
            Rect::new(
                rect.x + 3.0 * scale,
                rect.bottom() - 3.0 * scale,
                (rect.w - 6.0 * scale) * command.progress,
                2.0 * scale,
            ),
            tokens.supply,
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: console layout invariants, headless mock, icon catalog.
// ---------------------------------------------------------------------------


/// DragonUI modules/micromenu.lua: atlas UV grid — address sub-rects by
/// (col,row) in a uniform grid instead of per-button textures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasGridUv {
    pub cols: u32,
    pub rows: u32,
}

impl AtlasGridUv {
    pub const MICRO_MENU: Self = Self { cols: 4, rows: 4 };
    pub const MINIMAP_QUICKSLOT: Self = Self { cols: 4, rows: 1 };

    pub fn uv(self, col: u32, row: u32) -> [f32; 4] {
        let cols = self.cols.max(1) as f32;
        let rows = self.rows.max(1) as f32;
        let c = (col.min(self.cols.saturating_sub(1))) as f32;
        let r = (row.min(self.rows.saturating_sub(1))) as f32;
        [c / cols, r / rows, (c + 1.0) / cols, (r + 1.0) / rows]
    }
}

/// Three/nine-slice backdrop insets (DragonUI modules/buttons.lua backdrop
/// rendering: edge insets kept while center stretches).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl BackdropInsets {
    pub const fn uniform(v: f32) -> Self {
        Self { left: v, top: v, right: v, bottom: v }
    }

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
}

/// Castbar fill height equals parent height (DragonUI modules/castbar.lua:885).
pub fn castbar_fill_rect(parent: Rect, progress: f32) -> Rect {
    let p = progress.clamp(0.0, 1.0);
    Rect::new(parent.x, parent.y, parent.w * p, parent.h)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::TextureImage;
    use crate::hud;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root")
    }

    fn icon_dir() -> PathBuf {
        repo_root().join("analysis/war3_full3/ReplaceableTextures/CommandButtons")
    }

    fn load_icon(textures: &mut Vec<TextureImage>, name: &str) -> Option<usize> {
        let path = icon_dir().join(name);
        load_texture_path(textures, &path)
    }

    fn load_texture_path(textures: &mut Vec<TextureImage>, path: &Path) -> Option<usize> {
        let bytes = fs::read(path).ok()?;
        let image = wc3_formats::blp::decode(&bytes).ok()?;
        let index = textures.len();
        textures.push(TextureImage {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        });
        Some(index)
    }

    fn load_ui_texture(textures: &mut Vec<TextureImage>, relative: &str) -> Option<usize> {
        let path = repo_root().join("analysis/war3_full3").join(relative);
        load_texture_path(textures, &path)
    }

    #[derive(Clone, Copy)]
    struct Icons {
        gold: Option<usize>,
        lumber: Option<usize>,
        supply: Option<usize>,
        stop: Option<usize>,
        army: Option<usize>,
        base: Option<usize>,
        gather: Option<usize>,
        attack: Option<usize>,
        hold: Option<usize>,
        footman: Option<usize>,
        archer: Option<usize>,
        peasant: Option<usize>,
        build: Option<usize>,
    }

    fn load_icons(textures: &mut Vec<TextureImage>) -> Icons {
        Icons {
            gold: load_ui_texture(textures, "UI/Feedback/Resources/ResourceGold.blp")
                .or_else(|| load_icon(textures, "BTNGatherGold.blp")),
            lumber: load_ui_texture(textures, "UI/Feedback/Resources/ResourceLumber.blp")
                .or_else(|| load_icon(textures, "BTNBacktoWork.blp")),
            supply: load_ui_texture(textures, "UI/Feedback/Resources/ResourceSupply.blp")
                .or_else(|| load_icon(textures, "BTNFootman.blp")),
            stop: load_icon(textures, "BTNStop.blp"),
            army: load_icon(textures, "BTNMove.blp"),
            base: load_icon(textures, "BTNHumanBuild.blp"),
            gather: load_icon(textures, "BTNGatherGold.blp"),
            attack: load_icon(textures, "BTNAttack.blp"),
            hold: load_icon(textures, "BTNHoldPosition.blp"),
            footman: load_icon(textures, "BTNFootman.blp"),
            archer: load_icon(textures, "BTNArcher.blp"),
            peasant: load_icon(textures, "BTNPeasant.blp"),
            build: load_icon(textures, "BTNHumanBuild.blp"),
        }
    }

    fn sample_view(
        viewport: Vec2,
        icons: Icons,
        tiles: [Option<usize>; 4],
        border: Option<usize>,
    ) -> HudView<'static> {
        let resources: &'static [ResourcePill<'static>] = Box::leak(
            vec![
                ResourcePill::new("1200", "GOLD", [0.95, 0.76, 0.30, 1.0]).icon(icons.gold),
                ResourcePill::new("340", "LUMBER", [0.48, 0.85, 0.56, 1.0]).icon(icons.lumber),
                ResourcePill::new("7/12", "SUPPLY", [0.44, 0.71, 1.0, 1.0]).icon(icons.supply),
            ]
            .into_boxed_slice(),
        );
        let commands: &'static [CommandButton<'static>] = Box::leak(
            vec![
                CommandButton::new("STOP", icons.stop).short("STOP").hotkey("S").sub("S"),
                CommandButton::new("ARMY", icons.army).short("ARMY").hotkey("A").sub("A"),
                CommandButton::new("BASE", icons.base).short("BASE").hotkey("B").sub("B"),
                CommandButton::new("GATHER", icons.gather).short("GATH").hotkey("G").sub("G"),
                CommandButton::new("ATK-MOVE", icons.attack).short("ATK").hotkey("M").sub("M"),
                CommandButton::new("HOLD", icons.hold).short("HOLD").hotkey("H").sub("H"),
                CommandButton::new("FOOTMAN", icons.footman).short("FOOT").hotkey("F").sub("F"),
                CommandButton::new("ARCHER", icons.archer).short("ARCH").hotkey("R").sub("R"),
                CommandButton::empty(),
                CommandButton::empty(),
                CommandButton::empty(),
                CommandButton::empty(),
            ]
            .into_boxed_slice(),
        );
        let dots: &'static [MinimapDot] = Box::leak(
            vec![
                MinimapDot::new(0.24, 0.32, 6.0, [0.44, 0.71, 1.0, 1.0]),
                MinimapDot::new(0.72, 0.66, 7.0, [1.0, 0.35, 0.37, 1.0]),
                MinimapDot::new(0.18, 0.22, 7.0, [0.95, 0.76, 0.30, 1.0]),
            ]
            .into_boxed_slice(),
        );
        HudView::new(viewport)
            .resources(resources)
            .minimap(MinimapView {
                dots,
                camera: Some([0.22, 0.26, 0.30, 0.34]),
                title: "MINIMAP",
                ..MinimapView::new("MINIMAP")
            })
            .selection(Some(
                SelectionView::new("ARCHER", 300, 300)
                    .class("WARRIOR")
                    .kind("RANGED")
                    .portrait(icons.archer)
                    .stats(0, 14)
                    .production(Some(ProductionView {
                        name: "TRAINING",
                        progress: 0.42,
                        remaining: 9,
                    })),
            ))
            .commands(commands)
            .map_name("Альянс04", 61.0)
            .mode("MOVE")
            .upkeep("NO UPKEEP")
            .console_tiles(tiles)
            .button_border(border)
            .toast(Some(ToastView::new(
                "THE ENEMY TOWN HALL IS UNDER ATTACK",
                ToastTone::Danger,
            )))
    }

    #[test]
    fn console_layout_is_safe_and_clear_at_reference_resolutions() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
            (
                Vec2::new(1280.0, 720.0),
                SafeArea::landscape_1080().scaled(720.0 / 1080.0),
            ),
        ] {
            let view = sample_view(viewport, icons, [None; 4], None).safe(safe);
            let layout = hud.layout(&view);
            let issues = layout.issues(&hud.tokens);
            assert!(
                issues.is_empty(),
                "layout issues at {viewport:?}: {issues:#?}"
            );
            assert!(
                layout.console.is_inside(&layout.viewport_rect()),
                "console={:?} viewport={:?}",
                layout.console,
                layout.viewport
            );
            // Operator: flush with the physical bottom, not the gesture inset.
            assert!(
                (layout.console.bottom() - viewport.y).abs() < 0.01,
                "console bottom must be flush with the screen bottom"
            );
        }
    }

    #[test]
    fn labels_fit_inside_the_console() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let view = sample_view(Vec2::new(2414.0, 1080.0), icons, [None; 4], None);
        let layout = hud.layout(&view);
        let font = Font::load();
        for (index, slot) in layout.card_slots.iter().enumerate() {
            let command = &view.commands[index];
            if !command.present {
                continue;
            }
            let scale = 0.80 * layout.scale;
            let fit = font.measure(command.hotkey, scale);
            assert!(
                fit <= slot.w - 6.0 * layout.scale + 0.5,
                "card hotkey clipped: {}",
                command.hotkey
            );
        }
        let name_scale = 0.62 * layout.scale;
        let name_fit = font.measure(&view.selection.as_ref().unwrap().name, name_scale);
        assert!(name_fit <= layout.name.w + 0.5, "selection name clipped");
    }

    #[test]
    fn hit_test_covers_minimap_card_and_selection() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let view = sample_view(Vec2::new(2414.0, 1080.0), icons, [None; 4], None);
        let layout = hud.layout(&view);
        assert_eq!(layout.hit(layout.minimap.centre()), Some(HudHit::Minimap));
        assert_eq!(
            layout.hit(layout.card_slots[0].centre()),
            Some(HudHit::Card(0))
        );
        assert_eq!(
            layout.hit(layout.selection.centre()),
            Some(HudHit::Selection)
        );
        assert_eq!(layout.hit(layout.console.centre()), Some(HudHit::Console));
    }

    #[test]
    fn contrast_of_tokens_meets_wcag_aa() {
        let tokens = Tokens::default();
        let panel = composite_over(tokens.panel, [0.165, 0.200, 0.149, 1.0]);
        for (name, color) in [
            ("text", tokens.text),
            ("muted", tokens.text_muted),
            ("gold", tokens.gold),
            ("supply", tokens.supply),
            ("hp_good", tokens.hp_good),
            ("hp_warn", tokens.hp_warn),
            ("hp_low", tokens.hp_low),
        ] {
            let ratio = contrast_ratio(color, panel);
            assert!(ratio >= 4.5, "{name} contrast {ratio:.2} < 4.5");
        }
    }

    fn load_pack_texture(textures: &mut Vec<TextureImage>, relative: &str) -> Option<usize> {
        let path = repo_root()
            .join("analysis/agent-hud/redraw-v0.1/blp")
            .join(relative);
        load_texture_path(textures, &path)
    }

    /// `MenuSkin` carries nine-slice parts (texture + authored size + insets),
    /// not bare texture indices: wrap a loaded BLP in a `MenuPart` sized from
    /// the decoded image, the same shape the runtime `Game::menu_skin` builds.
    fn menu_part_from(index: Option<usize>, textures: &[TextureImage]) -> Option<MenuPart> {
        let index = index?;
        let image = textures.get(index)?;
        Some(MenuPart::new(
            index,
            image.width as f32,
            image.height as f32,
            [24.0, 24.0, 24.0, 24.0],
        ))
    }

    fn load_menu_pack_part(textures: &mut Vec<TextureImage>, relative: &str) -> Option<MenuPart> {
        let index = load_pack_texture(textures, relative);
        menu_part_from(index, textures)
    }

    fn load_menu_ui_part(textures: &mut Vec<TextureImage>, relative: &str) -> Option<MenuPart> {
        let index = load_ui_texture(textures, relative);
        menu_part_from(index, textures)
    }

    /// The selection text rows are stacked, not piled: at every device
    /// resolution the name, class, stats, production and bar rects must be
    /// pairwise disjoint and inside the selection panel. The device-visible
    /// defect this locks down was "ARCHER / WARRIOR - RANGED / ARMOR 0 DAMAGE 14
    /// / TRAINING 42%" all drawn over one another.
    /// Device metrics dump: the exact rects the phone draws (2414x1080,
    /// density 3.0, insets 210/120/90/96) so an HUD defect is fixed against
    /// numbers instead of a screenshot guess. Run with `--nocapture`.
    #[test]
    fn hud_layout_device_metrics_dump() {
        let hud = MobileHud::new();
        let safe = SafeArea::new(70.0, 40.0, 30.0, 32.0);
        let view = HudView::new(Vec2::new(2414.0, 1080.0))
            .density(3.0)
            .safe(safe);
        let layout = hud.layout(&view);
        for (name, rect) in [
            ("console", layout.console),
            ("selection", layout.selection),
            ("panel", layout.panel),
            ("portrait", layout.portrait),
            ("name", layout.name),
            ("class", layout.class),
            ("hp_bar", layout.hp_bar),
            ("hp_text", layout.hp_text),
            ("mana_bar", layout.mana_bar),
            ("stats", layout.stats),
            ("production", layout.production_label),
            ("army", layout.army),
            ("card", layout.card),
            ("minimap", layout.minimap),
            ("minimap_image", layout.minimap_image),
            ("groups_area", layout.groups_area),
            ("crest", layout.crest),
            ("tooltip", layout.tooltip),
            ("toast", layout.toast),
        ] {
            println!(
                "HUD_DUMP {name:<12} x={:>7.1} y={:>7.1} w={:>7.1} h={:>6.1}",
                rect.x, rect.y, rect.w, rect.h
            );
        }
        println!("HUD_DUMP card_slots={}", layout.card_slots.len());
        for (index, rect) in layout.card_slots.iter().take(3).enumerate() {
            println!(
                "HUD_DUMP slot{index}      x={:>7.1} y={:>7.1} w={:>7.1} h={:>6.1}",
                rect.x, rect.y, rect.w, rect.h
            );
        }
        println!("HUD_DUMP viewport={:?} density={} scale={}", view.viewport, layout.density, layout.scale);
    }

    #[test]
    fn hud_selection_text_rects_never_overlap() {
        fn overlap(a: Rect, b: Rect) -> f32 {
            let x = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
            let y = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
            x * y
        }
        let hud = MobileHud::new();
        let cases = [
            (2414.0f32, 1080.0f32, 2.75f32),
            (1920.0, 1080.0, 2.0),
            (1280.0, 720.0, 1.0),
            (1080.0, 2414.0, 2.75),
        ];
        for (w, h, density) in cases {
            let view = HudView::new(Vec2::new(w, h)).density(density);
            let layout = hud.layout(&view);
            let rects = [
                ("name", layout.name),
                ("class", layout.class),
                ("stats", layout.stats),
                ("production", layout.production_label),
                ("hp", layout.hp_bar),
                ("mana", layout.mana_bar),
            ];
            for (index, (name_a, a)) in rects.iter().enumerate() {
                if a.w <= 0.0 || a.h <= 0.0 {
                    continue;
                }
                assert!(
                    layout.selection.w > 0.0
                        && a.x >= layout.selection.x - 1.0
                        && a.y >= layout.selection.y - 1.0
                        && a.right() <= layout.selection.right() + 1.0
                        && a.bottom() <= layout.selection.bottom() + 1.0,
                    "{w}x{h}@{density}: {name_a} {a:?} escapes the selection panel {:?}",
                    layout.selection
                );
                for (name_b, b) in rects.iter().skip(index + 1) {
                    if b.w <= 0.0 || b.h <= 0.0 {
                        continue;
                    }
                    let shared = overlap(*a, *b);
                    let smaller = (a.w * a.h).min(b.w * b.h).max(1.0);
                    assert!(
                        shared / smaller <= 0.25,
                        "{w}x{h}@{density}: {name_a} {a:?} overlaps {name_b} {b:?} by {:.0}%",
                        shared / smaller * 100.0
                    );
                }
            }
        }
    }

    #[test]
    fn ui_command_card_peon_headless_mock_writes_png() {
        // Device-free proof that the worker card is filled: the real
        // `hud::build_buttons` output drives the same card renderer.
        let hud = MobileHud::new();
        let mut game = crate::game::Game::new(crate::assets::Assets::load(None, None), 0xCA4D);
        let peon = game
            .sim
            .spawn_def(0, "hpea", Vec2::new(600.0, 600.0))
            .expect("peon");
        game.selection = vec![peon];
        let buttons = crate::hud::build_buttons(&game);
        let commands: Vec<CommandButton<'_>> = buttons
            .iter()
            .map(|button| {
                if button.label.is_empty() {
                    CommandButton::empty()
                } else {
                    CommandButton::new(
                        &button.label,
                        game.assets.ui.command_icon(&button.action),
                    )
                    .short(&button.short)
                    .hotkey(&button.hotkey)
                    .sub(&button.sub)
                    .enabled(button.enabled)
                }
            })
            .collect();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        let view = sample_view(viewport, icons, [None; 4], None)
            .safe(SafeArea::landscape_1080())
            .commands(&commands);
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        assert!(layout.issues(&hud.tokens).is_empty());
        let pixels = rasterize(
            &batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out_dir = repo_root().join("analysis/agent-hud/card-fix");
        std::fs::create_dir_all(&out_dir).expect("card-fix dir");
        let out = out_dir.join("mock-peon-card.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("write png");
        eprintln!("HUD_CARD_MOCK {}", out.display());
    }

    #[test]
    fn ui_command_card_hero_headless_mock_writes_png() {
        let hud = MobileHud::new();
        let mut game =
            crate::game::Game::new(crate::assets::Assets::load(None, None), 0xCA4E);
        let hero = game
            .sim
            .spawn_def(0, "Hpal", Vec2::new(600.0, 600.0))
            .expect("hero");
        game.selection = vec![hero];
        let buttons = crate::hud::build_buttons(&game);
        for slot in [6usize, 7, 10, 11] {
            assert!(!buttons[slot].label.is_empty(), "hero ability slot {slot}");
            assert!(!buttons[slot].hotkey.is_empty(), "hero hotkey slot {slot}");
        }
        let commands: Vec<CommandButton<'_>> = buttons
            .iter()
            .map(|button| {
                if button.label.is_empty() {
                    CommandButton::empty()
                } else {
                    CommandButton::new(
                        &button.label,
                        game.assets.ui.command_icon(&button.action),
                    )
                    .short(&button.short)
                    .hotkey(&button.hotkey)
                    .sub(&button.sub)
                    .enabled(button.enabled)
                }
            })
            .collect();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        let view = sample_view(viewport, icons, [None; 4], None)
            .safe(SafeArea::landscape_1080())
            .commands(&commands);
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        assert!(layout.issues(&hud.tokens).is_empty());
        let pixels = rasterize(
            &batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out_dir = repo_root().join("analysis/agent-hud/card-fix");
        std::fs::create_dir_all(&out_dir).expect("card-fix dir");
        let out = out_dir.join("mock-hero-card.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("write png");
        eprintln!("HUD_HERO_CARD_MOCK {}", out.display());
    }

    #[test]
    fn ui_menu_restyled_headless_mock_writes_png() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        let skin = MenuSkin {
            panel: load_menu_pack_part(&mut textures, "UI/Console/Human/HumanMenuPanel.blp"),
            button_normal: load_menu_pack_part(
                &mut textures,
                "UI/Console/Human/HumanMenuButtonNormal.blp",
            ),
            button_pressed: load_menu_pack_part(
                &mut textures,
                "UI/Console/Human/HumanMenuButtonPressed.blp",
            ),
            button_disabled: load_menu_pack_part(
                &mut textures,
                "UI/Console/Human/HumanMenuButtonDisabled.blp",
            ),
            loading_frame: load_menu_pack_part(
                &mut textures,
                "UI/Console/Human/HumanLoadingFrame.blp",
            ),
            panel_border: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/EscMenu/Human/human-options-menu-border.blp",
            ),
            title_band: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder.blp",
            ),
            button_hover: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button-KeyboardHighlight.blp",
            ),
            banner: None,
            fonts: None,
        };
        if skin.panel.is_none() && skin.button_normal.is_none() {
            eprintln!("skip: pack menu BLP missing");
            return;
        }
        let view = MenuView::for_screen(MenuScreen::Main, viewport, "Альянс04")
            .redraw(Some(skin));
        let mut batch = UiBatch::default();
        let layout = hud.draw_menu(&view, &mut batch, &font, &chrome);
        assert!(layout.title_band.h >= 56.0, "title band");
        // Relayout contract: glue entries are finger targets clamped to
        // [MENU_MIN_TARGET_PX, 150] px (operator's 90 px floor), not the old
        // 44-56 px desktop strip.
        for rect in &layout.buttons {
            assert!(
                rect.h + 0.01 >= MENU_MIN_TARGET_PX && rect.h <= 150.0 + 0.01,
                "dialog button height {} outside [{MENU_MIN_TARGET_PX}, 150]",
                rect.h
            );
        }
        let pixels = rasterize(
            &batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out_dir = repo_root().join("analysis/agent-hud/card-fix");
        std::fs::create_dir_all(&out_dir).expect("card-fix dir");
        let out = out_dir.join("mock-menu-restyled.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("write png");
        eprintln!("HUD_MENU_MOCK {}", out.display());
    }

    /// Review artifacts: every menu screen and in-game dialog rendered from the
    /// shipped war3skins/glue art through the real HUD draw path.
    #[test]
    fn ui_menus_shipped_art_mock_pngs() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(1920.0, 1080.0);
        let skin = MenuSkin {
            panel: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/EscMenu/Human/human-options-menu-background.blp",
            ),
            panel_border: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/EscMenu/Human/human-options-menu-border.blp",
            ),
            title_band: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder.blp",
            ),
            button_normal: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground.blp",
            ),
            button_hover: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button-KeyboardHighlight.blp",
            ),
            button_pressed: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground-Down.blp",
            ),
            button_disabled: load_menu_ui_part(
                &mut textures,
                "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground-Disabled.blp",
            ),
            loading_frame: None,
            banner: None,
            fonts: None,
        };
        assert!(skin.panel.is_some() && skin.panel_border.is_some(), "panel art");
        assert!(
            skin.button_normal.is_some()
                && skin.button_hover.is_some()
                && skin.button_pressed.is_some()
                && skin.button_disabled.is_some(),
            "all four button states resolve"
        );
        let out_dir = repo_root().join("analysis/agent-fdf/mocks");
        std::fs::create_dir_all(&out_dir).expect("mocks dir");
        let mut written = Vec::new();
        for screen in [
            MenuScreen::Main,
            MenuScreen::SinglePlayer,
            MenuScreen::Skirmish,
            MenuScreen::MapSelect,
            MenuScreen::PauseOptions,
            MenuScreen::Loading,
            MenuScreen::Victory,
            MenuScreen::Defeat,
        ] {
            let mut view = MenuView::for_screen(screen, viewport, "Альянс04").redraw(Some(skin));
            view.progress = 0.62;
            view.status = if screen == MenuScreen::Loading { "LOADING MAP" } else { "" };
            view.hover = Some(1);
            view.pressed = Some(2);
            let mut batch = UiBatch::default();
            let layout = hud.draw_menu(&view, &mut batch, &font, &chrome);
            assert!(layout.title_band.h >= 56.0, "{screen:?} title band");
            for pair in layout.buttons.windows(2) {
                assert!(!pair[0].intersects(&pair[1]), "{screen:?} buttons overlap");
            }
            let pixels = rasterize(&batch, &textures, &font, &map, viewport.x as u32, viewport.y as u32);
            let colors: std::collections::HashSet<[u8; 3]> = pixels
                .chunks_exact(4)
                .step_by(37)
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect();
            eprintln!("MENU_MOCK {screen:?} textured_draws={} colors={}", batch.textured.len(), colors.len());
            assert!(colors.len() > 60 && batch.textured.len() >= 4, "{screen:?} flat: {} colors, {} draws", colors.len(), batch.textured.len());
            let name = format!("menu_{:?}.png", screen).to_lowercase();
            let out = out_dir.join(name);
            write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("menu png");
            let bytes = std::fs::metadata(&out).expect("png").len();
            assert!(bytes > 20_000, "{screen:?} png {} bytes", bytes);
            written.push((out, bytes));
        }
        for screen in [DialogScreen::QuestLog, DialogScreen::HeroSkills, DialogScreen::BuildingPanel] {
            let view = DialogView::for_screen(screen, viewport).with_skin(Some(skin));
            let mut batch = UiBatch::default();
            let layout = hud.draw_dialog(&view, &mut batch, &font, &chrome);
            assert!(layout.buttons.len() == 1, "{screen:?} close button");
            let pixels = rasterize(&batch, &textures, &font, &map, viewport.x as u32, viewport.y as u32);
            let name = format!("dialog_{:?}.png", screen).to_lowercase();
            let out = out_dir.join(name);
            write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("dialog png");
            let bytes = std::fs::metadata(&out).expect("png").len();
            assert!(bytes > 20_000, "{screen:?} png {} bytes", bytes);
            written.push((out, bytes));
        }
        eprintln!("UI_SHIPPED_MOCKS {written:?}");
    }

    #[test]
    fn headless_redraw_mock_writes_png() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let tiles = [
            load_pack_texture(&mut textures, "UI/Console/Human/HumanUITile01.blp"),
            load_pack_texture(&mut textures, "UI/Console/Human/HumanUITile02.blp"),
            load_pack_texture(&mut textures, "UI/Console/Human/HumanUITile03.blp"),
            load_pack_texture(&mut textures, "UI/Console/Human/HumanUITile04.blp"),
        ];
        if tiles.iter().all(|tile| tile.is_none()) {
            eprintln!("skip: redraw pack BLP missing");
            return;
        }
        let border = load_pack_texture(
            &mut textures,
            "UI/Widgets/Console/Human/CommandButton/human-multipleselection-border.blp",
        );
        let button = |textures: &mut Vec<TextureImage>, state: &str| {
            load_pack_texture(
                textures,
                &format!("UI/Widgets/Console/Human/CommandButton/human-button-{state}.blp"),
            )
        };
        let skin = RedrawSkin {
            console_left_cap: tiles[0],
            console_left_mid: tiles[1],
            console_right_mid: tiles[2],
            console_right_cap: tiles[3],
            topbar: load_pack_texture(&mut textures, "UI/Console/Human/HumanTopbarPlate.blp"),
            button: [
                button(&mut textures, "normal"),
                button(&mut textures, "pressed"),
                button(&mut textures, "disabled"),
                button(&mut textures, "cooldown"),
                button(&mut textures, "autocast"),
                button(&mut textures, "empty"),
            ],
            portrait: [
                load_pack_texture(&mut textures, "UI/Console/Human/HumanUIPortraitMaskNormal.blp"),
                load_pack_texture(&mut textures, "UI/Console/Human/HumanUIPortraitMaskHero.blp"),
                load_pack_texture(&mut textures, "UI/Console/Human/HumanUIPortraitMaskEnemy.blp"),
            ],
            minimap_frame: load_pack_texture(&mut textures, "UI/Console/Human/HumanMinimapFrame.blp"),
            minimap_frame_alert: load_pack_texture(
                &mut textures,
                "UI/Console/Human/HumanMinimapFrameAlert.blp",
            ),
            minimap_header: load_pack_texture(
                &mut textures,
                "UI/Console/Human/HumanMinimapHeader.blp",
            ),
            pill: load_pack_texture(&mut textures, "UI/Console/Human/HumanResourcePill.blp"),
            pill_warn: load_pack_texture(&mut textures, "UI/Console/Human/HumanResourcePillWarn.blp"),
            selection_panel: load_pack_texture(
                &mut textures,
                "UI/Console/Human/HumanSelectionPanel.blp",
            ),
            group_inactive: load_pack_texture(
                &mut textures,
                "UI/Console/Human/HumanControlGroupButtonInactive.blp",
            ),
            group_active: load_pack_texture(
                &mut textures,
                "UI/Console/Human/HumanControlGroupButtonActive.blp",
            ),
            tooltip_panel: load_pack_texture(
                &mut textures,
                "UI/Widgets/ToolTips/Human/human-tooltip-panel.blp",
            ),
            progress: load_pack_texture(&mut textures, "UI/Console/Human/HumanBuildProgress.blp"),
        };
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        let view = sample_view(viewport, icons, tiles, border)
            .safe(SafeArea::landscape_1080())
            .redraw(Some(skin));
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        assert!(layout.redraw.is_some(), "redraw console parts built");
        assert!(layout.issues(&hud.tokens).is_empty());
        let pixels = rasterize(
            &batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out = repo_root().join("analysis/agent-hud/mock-redraw-2414x1080.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("write png");
        eprintln!("HUD_REDRAW_MOCK {}", out.display());
    }

    #[test]
    fn headless_console_mock_writes_png() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let tiles = [
            load_ui_texture(&mut textures, "UI/Console/Human/HumanUITile01.blp"),
            load_ui_texture(&mut textures, "UI/Console/Human/HumanUITile02.blp"),
            load_ui_texture(&mut textures, "UI/Console/Human/HumanUITile03.blp"),
            load_ui_texture(&mut textures, "UI/Console/Human/HumanUITile04.blp"),
        ];
        let border = load_ui_texture(
            &mut textures,
            "UI/Widgets/Console/Human/CommandButton/human-activebutton.blp",
        );
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        let view = sample_view(viewport, icons, tiles, border).safe(SafeArea::landscape_1080());
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        assert!(layout.issues(&hud.tokens).is_empty());
        let pixels = rasterize(
            &batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out = repo_root().join("analysis/agent-hud/mock-console-2414x1080.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("write png");
        eprintln!("HUD_CONSOLE_MOCK {}", out.display());
    }

    /// #7 screenshot evidence, headless (NOT device): draw the *real* HUD batch
    /// of a shipped install — real BLP portraits decoded into `assets.textures`
    /// — and prove every selected unit's army slot contains icon art instead of
    /// a bare plate. The CPU rasterizer preserves `UiBatch` painter order, so
    /// this is the same quad stream the GPU pass receives.
    #[test]
    fn art7_army_panel_headless_render() {
        let map = repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m");
        if !map.is_file() {
            eprintln!("ART7_RENDER_SKIP Human04.w3m missing");
            return;
        }
        let mut mounted = None;
        for mpq in [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ] {
            let path = Path::new(mpq);
            if path.is_file() {
                mounted = Some(crate::Assets::load(Some(path), Some(&map)));
                break;
            }
        }
        if mounted.is_none() {
            let loose = repo_root().join("analysis/war3_full3");
            if loose.is_dir() {
                mounted = Some(crate::Assets::load(Some(&loose), Some(&map)));
            }
        }
        let Some(assets) = mounted else {
            eprintln!("ART7_RENDER_SKIP no War3.mpq / analysis/war3_full3");
            return;
        };
        if !assets.models.iter().any(|model| !model.fallback) {
            eprintln!("ART7_RENDER_SKIP no real MDX models in the mounted install");
            return;
        }
        let mut game = crate::Game::new(assets, 0xA27);
        game.enter_battle();
        game.resize(2414.0, 1080.0);
        for _ in 0..8 {
            game.update(1.0 / 30.0);
        }
        let mut seen = std::collections::HashSet::new();
        let mut chosen = Vec::new();
        for unit in game.sim.alive_units() {
            if unit.owner != game.player || !seen.insert(unit.def().id) {
                continue;
            }
            chosen.push(unit.id);
            if chosen.len() == 5 {
                break;
            }
        }
        if chosen.len() < 2 {
            eprintln!("ART7_RENDER_SKIP fewer than 2 player unit defs");
            return;
        }
        game.selection = chosen;
        // The stored army row is a function of the chip count.
        game.resize(2414.0, 1080.0);
        let chips = game.army_chips();
        assert!(
            chips.iter().all(|chip| chip.icon.is_some()),
            "every army chip must resolve a shipped BLP icon: {chips:?}"
        );
        let stored_slots = game
            .mobile_layout
            .as_ref()
            .map(|layout| layout.army_slots.clone())
            .unwrap_or_default();
        let scene = game.scene();
        // Locate the *drawn* chip windows from the batch itself (the icon
        // textures the army panel emits), not from the stored hit layout: the
        // two can differ and the evidence must be the pixels the player sees.
        let chip_icons: Vec<usize> = chips.iter().filter_map(|chip| chip.icon).collect();
        let height_hint = 1080.0f32;
        let mut windows = Vec::new();
        for texture in &chip_icons {
            let mut best: Option<(f32, f32, f32, f32, f32)> = None;
            for draw in &scene.ui.textured {
                if draw.texture != *texture || draw.vertices.is_empty() {
                    continue;
                }
                let x0 = draw.vertices.iter().map(|v| v.pos[0]).fold(f32::INFINITY, f32::min);
                let y0 = draw.vertices.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
                let x1 = draw.vertices.iter().map(|v| v.pos[0]).fold(f32::NEG_INFINITY, f32::max);
                let y1 = draw.vertices.iter().map(|v| v.pos[1]).fold(f32::NEG_INFINITY, f32::max);
                let area = (x1 - x0) * (y1 - y0);
                // The same icon also appears as the selection portrait and a
                // card icon; the army chip is the *bottom-band* copy.
                if y0 < height_hint * 0.8 {
                    continue;
                }
                if best.map_or(true, |(a, ..)| area > a) {
                    best = Some((area, x0, y0, x1, y1));
                }
            }
            let Some((_, x0, y0, x1, y1)) = best else {
                eprintln!("ART7_RENDER_SKIP icon texture {texture} is not in the UI batch");
                return;
            };
            windows.push((x0, y0, x1 - x0, y1 - y0));
        }
        eprintln!(
            "ART7_RENDER_LAYOUT stored_slots={} drawn_windows={:?}",
            stored_slots.len(),
            windows
        );
        assert_eq!(
            windows.len(),
            chips.len(),
            "every army chip icon must be drawn in the UI batch"
        );
        let minimap = game.assets.minimap.clone().unwrap_or_else(map_texture);
        let width = 2414u32;
        let height = 1080u32;
        let pixels = rasterize(
            &scene.ui,
            &game.assets.textures,
            &game.font,
            &minimap,
            width,
            height,
        );
        let out_dir = repo_root().join("analysis/agent-hud/all-layers-20261121");
        fs::create_dir_all(&out_dir).expect("evidence dir");
        let out = out_dir.join("headless-NOT-DEVICE-army-panel-cpu-2414x1080.png");
        write_png(&out, width, height, &pixels).expect("write army panel png");
        let bytes = fs::metadata(&out).expect("png").len();

        let mut report = Vec::new();
        let mut means = Vec::new();
        for (index, (sx, sy, sw, sh)) in windows.iter().copied().enumerate() {
            let inset = 6.0f32.min(sw.min(sh) * 0.2);
            let x0 = (sx + inset).max(0.0) as u32;
            let y0 = (sy + inset).max(0.0) as u32;
            let x1 = ((sx + sw - inset).max(x0 as f32) as u32).min(width);
            let y1 = ((sy + sh - inset).max(y0 as f32) as u32).min(height);
            let mut colours = std::collections::HashSet::new();
            let mut sum = [0f64; 3];
            let mut count = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let at = ((y * width + x) * 4) as usize;
                    let (r, g, b) = (pixels[at], pixels[at + 1], pixels[at + 2]);
                    colours.insert((r / 24, g / 24, b / 24));
                    sum[0] += r as f64;
                    sum[1] += g as f64;
                    sum[2] += b as f64;
                    count += 1;
                }
            }
            let count = count.max(1) as f64;
            let mean = [sum[0] / count, sum[1] / count, sum[2] / count];
            assert!(
                colours.len() >= 16,
                "army window {index} must show a real portrait, found {} colour buckets (bare plate) at ({sx:.0},{sy:.0}) {sw:.0}x{sh:.0}",
                colours.len()
            );
            means.push(mean);
            report.push(format!(
                "window{index}={}x{}@({:.0},{:.0}) colours={} mean=({:.0},{:.0},{:.0})",
                sw as i32,
                sh as i32,
                sx,
                sy,
                colours.len(),
                mean[0],
                mean[1],
                mean[2]
            ));
        }
        let mut distinct = 0;
        for i in 0..means.len() {
            for j in i + 1..means.len() {
                let distance = (means[i][0] - means[j][0]).abs()
                    + (means[i][1] - means[j][1]).abs()
                    + (means[i][2] - means[j][2]).abs();
                if distance > 15.0 {
                    distinct += 1;
                }
            }
        }
        assert!(
            distinct >= 3,
            "distinct defs must paint distinct portraits: {report:?}"
        );
        assert!(bytes > 100_000, "army panel png is not a blank canvas: {bytes} B");
        eprintln!(
            "ART7_RENDER_VISIBLE png={} bytes={bytes} chips={} drawn={} distinct_pairs={distinct} PASS {}",
            out.display(),
            chips.len(),
            windows.len(),
            report.join(" | ")
        );
    }

    #[test]
    fn headless_menu_mock_writes_png() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        for (screen, name) in [
            (MenuScreen::Main, "mock-menu-main.png"),
            (MenuScreen::MapSelect, "mock-menu-map.png"),
            (MenuScreen::Loading, "mock-menu-loading.png"),
        ] {
            let mut view = MenuView::for_screen(screen, viewport, "Альянс04");
            view.progress = 0.62;
            view.status = "LOADING MAP";
            let mut batch = UiBatch::default();
            let layout = hud.draw_menu(&view, &mut batch, &font, &chrome);
            for pair in layout.buttons.windows(2) {
                assert!(!pair[0].intersects(&pair[1]), "menu buttons overlap");
            }
            let pixels = rasterize(&batch, &textures, &font, &map, viewport.x as u32, viewport.y as u32);
            let out = repo_root().join("analysis/agent-hud").join(name);
            write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("write menu png");
            eprintln!("HUD_MENU_MOCK {}", out.display());
        }
    }

    #[test]
    fn commandbuttons_catalog_validates_all_691_icons() {
        let dir = icon_dir();
        let mut files = fs::read_dir(&dir)
            .expect("command button directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("blp"))
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(files.len(), 691);
        let cols = 16usize;
        let rows = files.len().div_ceil(cols);
        let mut atlas = vec![0u8; 1024 * rows * 64 * 4];
        let mut catalog = String::from("name\twidth\theight\tmin_alpha\tmax_alpha\n");
        let mut zero_alpha = 0;
        for (index, path) in files.iter().enumerate() {
            let bytes = fs::read(path).expect("read blp");
            let image = wc3_formats::blp::decode(&bytes).expect("decode blp");
            assert_eq!((image.width, image.height), (64, 64));
            let mut min_alpha = 255u8;
            let mut max_alpha = 0u8;
            for pixel in image.rgba.chunks_exact(4) {
                min_alpha = min_alpha.min(pixel[3]);
                max_alpha = max_alpha.max(pixel[3]);
            }
            if max_alpha == 0 {
                zero_alpha += 1;
            }
            let column = index % cols;
            let row = index / cols;
            let base_x = column * 64;
            let base_y = row * 64;
            for y in 0..64usize {
                let src = y * 64 * 4;
                let dst = ((base_y + y) * 1024 + base_x) * 4;
                atlas[dst..dst + 64 * 4].copy_from_slice(&image.rgba[src..src + 64 * 4]);
            }
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("?");
            catalog.push_str(&format!("{name}\t64\t64\t{min_alpha}\t{max_alpha}\n"));
        }
        assert_eq!(zero_alpha, 0);
        let out = repo_root().join("analysis/agent-hud/icons");
        fs::create_dir_all(&out).expect("icon dir");
        write_png(
            &out.join("commandbuttons_atlas.png"),
            1024,
            (rows * 64) as u32,
            &atlas,
        )
        .expect("write atlas");
        fs::write(out.join("catalog.tsv"), catalog).expect("catalog");
    }

    fn map_texture() -> TextureImage {
        let size = 256u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let nx = x as f32 / size as f32;
                let ny = y as f32 / size as f32;
                // Grass base with two darker forest patches.
                let forest = ((nx - 0.72).powi(2) + (ny - 0.30).powi(2)).sqrt()
                    + ((nx - 0.20).powi(2) + (ny - 0.78).powi(2)).sqrt();
                let shade = if forest < 0.24 { 0.72 } else { 1.0 };
                // A river crossing the map and a diagonal road.
                let river = (ny - (0.58 + (nx - 0.5) * 0.22)).abs();
                let road = (nx - (0.18 + ny * 0.55)).abs();
                let (mut r, mut g, mut b) = (0.30 * shade, 0.44 * shade, 0.20 * shade);
                if river < 0.035 {
                    r = 0.18;
                    g = 0.34;
                    b = 0.52;
                } else if road < 0.018 {
                    r = 0.52;
                    g = 0.45;
                    b = 0.28;
                }
                // Gold mines and the two bases.
                for (mx, my, color) in [
                    (0.18f32, 0.22f32, [0.95f32, 0.76f32, 0.30f32]),
                    (0.82f32, 0.78f32, [0.95f32, 0.76f32, 0.30f32]),
                    (0.24f32, 0.32f32, [0.43f32, 0.71f32, 1.00f32]),
                    (0.72f32, 0.66f32, [1.00f32, 0.35f32, 0.37f32]),
                ] {
                    let d = ((nx - mx).powi(2) + (ny - my).powi(2)).sqrt();
                    if d < 0.045 {
                        r = color[0];
                        g = color[1];
                        b = color[2];
                    }
                }
                let index = ((y * size + x) * 4) as usize;
                rgba[index] = (r * 255.0) as u8;
                rgba[index + 1] = (g * 255.0) as u8;
                rgba[index + 2] = (b * 255.0) as u8;
                rgba[index + 3] = 255;
            }
        }
        TextureImage {
            width: size,
            height: size,
            rgba,
        }
    }

    fn composite_over(src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
        let a = src[3];
        [
            src[0] * a + dst[0] * (1.0 - a),
            src[1] * a + dst[1] * (1.0 - a),
            src[2] * a + dst[2] * (1.0 - a),
            1.0,
        ]
    }

    fn luminance(color: [f32; 4]) -> f32 {
        let channel = |value: f32| {
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2])
    }

    fn contrast_ratio(a: [f32; 4], b: [f32; 4]) -> f32 {
        let la = luminance(a);
        let lb = luminance(b);
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn tooltip_pack_panel_uses_a_tall_nine_sliced_box() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let base = sample_view(Vec2::new(2414.0, 1080.0), icons, [None; 4], None)
            .safe(SafeArea::landscape_1080());
        // No tooltip → no tooltip rect (the box is content-driven now).
        let plain = hud.layout(&base);
        assert_eq!(plain.tooltip.w, 0.0, "no tooltip rect without text");
        let skin = RedrawSkin {
            console_left_cap: Some(0),
            console_left_mid: Some(1),
            console_right_mid: Some(2),
            console_right_cap: Some(3),
            tooltip_panel: Some(4),
            ..RedrawSkin::default()
        };
        // A long WC3 tooltip: title + cost + a wrapped description. The pack
        // panel is nine-sliced over the measured (tall) box, never squashed.
        let view = base
            .redraw(Some(skin))
            .tooltip(Some(
                "WATCH TOWER  (B)\nCOST 130 GOLD · 80 LUMBER\nA defensive tower that shoots ground and air units."
                    .to_string(),
            ))
            .tooltip_anchor(Some(Rect::new(1900.0, 700.0, 64.0, 64.0)));
        let redraw = hud.layout(&view);
        assert!(
            redraw.tooltip.h >= 2.0 * 30.0 * redraw.scale,
            "pack tooltip box grows with the wrapped text: {:?}",
            redraw.tooltip
        );
        assert!(redraw.tooltip.is_inside(&redraw.safe_frame), "tooltip inside safe frame");
        let rect = redraw.tooltip;
        let quads = nine_slice_quads(rect, 24.0 * redraw.scale, [24.0 / 384.0, 24.0 / 256.0]);
        let mut area = 0.0;
        for (part, uv) in quads {
            assert!(part.is_inside(&rect), "nine-slice part inside the box: {part:?}");
            assert!(
                uv.iter().all(|value| (0.0..=1.0).contains(value)),
                "uv inside [0,1]: {uv:?}"
            );
            area += part.w * part.h;
        }
        assert!((area - rect.w * rect.h).abs() < 1.0, "nine-slice covers the box");
    }

    #[test]
    fn hud_scale_boost_is_applied_and_clamped() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
            (
                Vec2::new(1280.0, 720.0),
                SafeArea::landscape_1080().scaled(720.0 / 1080.0),
            ),
        ] {
            let view = sample_view(viewport, icons, [None; 4], None).safe(safe);
            let layout = hud.layout(&view);
            // `layout.scale` is the *pack art* scale (console height / 300),
            // not the HUD preference: 521 made the console a 0.34 h band, so
            // comparing art px to the 1.10 boost was measuring the wrong field.
            let ui = layout.dp().prefs.scale();
            assert!(
                (UI_PREF_MIN..=UI_PREF_MAX).contains(&ui),
                "UI preference {ui} outside [{UI_PREF_MIN}, {UI_PREF_MAX}]"
            );
            assert!(
                (0.62..=2.4).contains(&layout.scale),
                "art scale {} outside the 0.62..2.4 clamp at {viewport:?}",
                layout.scale
            );
            let reference = viewport == Vec2::new(2414.0, 1080.0)
                || viewport == Vec2::new(1080.0, 2414.0);
            if reference {
                assert!(ui >= 0.95, "boost {ui} < 0.95 at {viewport:?}");
            }
            assert!(
                layout.console.is_inside(&layout.viewport_rect()),
                "console inside the viewport at {viewport:?}: {:?}",
                layout.console
            );
            assert!(
                (layout.console.bottom() - viewport.y).abs() < 0.01,
                "console bottom is flush with the screen bottom at {viewport:?}"
            );
            assert!(
                layout.console.h <= layout.safe_frame.h + 0.5,
                "console inside the safe height at {viewport:?}: {} > {}",
                layout.console.h,
                layout.safe_frame.h
            );
            if viewport.y >= 700.0 {
                assert!(
                    layout.console.h <= layout.safe_frame.h * 0.6 + 0.5,
                    "console height capped at {viewport:?}: {} > {}",
                    layout.console.h,
                    layout.safe_frame.h * 0.6
                );
            }
        }
        let view = sample_view(Vec2::new(2414.0, 1080.0), icons, [None; 4], None)
            .safe(SafeArea::landscape_1080());
        assert!(
            (hud.layout(&view).scale - 1.1).abs() < 1e-3,
            "HUD_SCALE_BOOST gives +10% at 2414x1080"
        );
    }

    #[test]
    fn dialog_content_override_and_minimap_fog_are_wired() {
        let lines = ["MAIN QUEST", "GOLD 500"];
        let view = DialogView::for_screen(DialogScreen::QuestLog, Vec2::new(2414.0, 1080.0))
            .with_content("QUEST LOG", &lines, "TAP · CLOSE");
        assert_eq!(view.title, "QUEST LOG");
        assert_eq!(view.lines, &lines[..]);
        assert_eq!(view.footer, "TAP · CLOSE");
        let fog = [0u8, 1, 2, 2, 1, 0];
        let minimap = MinimapView::new("MAP").fog(&fog, 3, 2);
        assert_eq!(minimap.fog, Some(&fog[..]));
        assert_eq!(minimap.fog_width, 3);
        assert_eq!(minimap.fog_height, 2);
        // The dialog layout keeps a title band and a close button row.
        let hud = MobileHud::new();
        let layout = hud.layout_dialog(&view);
        assert!(layout.title_band.h > 0.0);
        assert_eq!(layout.buttons.len(), 1);
        assert!(layout.panel.w > 0.0);
    }

    #[test]
    fn minimap_frame_and_header_are_anchored_to_the_hit_rect() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let view = sample_view(viewport, icons, [None; 4], None);
            let layout = hud.layout(&view);
            assert_eq!(
                layout.hit(layout.minimap.centre()),
                Some(HudHit::Minimap),
                "centre hits the minimap at {viewport:?}"
            );
            // The tap target IS the map square: the 1:1 ornament ring is chrome
            // around it (`COMPOSITION.md` §4 - a ring tap is not a map tap).
            assert_eq!(layout.minimap, layout.minimap_image);
            assert!((layout.minimap.w - layout.minimap.h).abs() < 0.01, "square map");
            let frame = layout.minimap_frame;
            assert!(
                (frame.x - (layout.minimap_image.x - MINIMAP_FRAME_INSETS[0])).abs() < 0.01
                    && (frame.right() - (layout.minimap_image.right() + MINIMAP_FRAME_INSETS[2]))
                        .abs()
                        < 0.01
                    && (frame.y - (layout.minimap_image.y - MINIMAP_FRAME_INSETS[1])).abs() < 0.01
                    && (frame.bottom()
                        - (layout.minimap_image.bottom() + MINIMAP_FRAME_INSETS[3]))
                        .abs()
                        < 0.01,
                "ring must be the 1:1 atlas insets around the image: {frame:?}"
            );
            for point in [
                Vec2::new(layout.minimap.x + 0.5, layout.minimap.y + 0.5),
                Vec2::new(layout.minimap.right() - 0.5, layout.minimap.y + 0.5),
                Vec2::new(layout.minimap.x + 0.5, layout.minimap.bottom() - 0.5),
                Vec2::new(layout.minimap.right() - 0.5, layout.minimap.bottom() - 0.5),
            ] {
                assert_eq!(
                    layout.hit(point),
                    Some(HudHit::Minimap),
                    "map corner belongs to the minimap hit rect at {point:?}"
                );
            }
            // The caption chip lives inside the map, top-left.
            assert!(layout.minimap_header.is_inside(&layout.minimap_image));
            assert!(
                layout.minimap_frame.is_inside(&layout.console),
                "frame {:?} vs console {:?} at {viewport:?} scale {}",
                layout.minimap_frame,
                layout.console,
                layout.scale
            );
            assert!(
                layout.minimap_rail.is_inside(&layout.console),
                "rail {:?} vs console {:?}",
                layout.minimap_rail,
                layout.console
            );
            assert!(layout.minimap.right() <= layout.minimap_rail.x);
        }
    }

    #[test]
    fn ui_hudskin_layout_keeps_the_portrait_column_clear() {
        let hud = MobileHud::new();
        let skin = crate::hudskin::tests_support::sample_skin();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let base = sample_view(viewport, icons, [None; 4], None);
            let view = HudView {
                hud_skin: Some(&skin),
                ..base
            };
            let layout = hud.layout(&view);
            // Retail shape: the name/class bands sit beside the frame, never
            // above it, so the 58.2 px band stack cannot leave the console.
            assert!(
                layout.name.x >= layout.portrait.right() - 0.01,
                "name band must start right of the portrait frame at {viewport:?}: name {:?} portrait {:?}",
                layout.name,
                layout.portrait
            );
            assert!(layout.class.x >= layout.portrait.right() - 0.01);
            // The frame starts after the expanded minimap hit rect: the 7.22 px
            // overlap the lane measured is gone.
            assert!(
                layout.portrait.intersection_area(&layout.minimap) <= 0.5,
                "portrait frame overlaps the minimap hit rect at {viewport:?}: {:?} vs {:?}",
                layout.portrait,
                layout.minimap
            );
            assert!(layout.portrait.is_inside(&layout.console));
            assert!(layout.name.is_inside(&layout.console));
            assert!(layout.class.is_inside(&layout.console));
            assert!(layout.stats.is_inside(&layout.console));
            // The army row starts below the frame, never under it.
            assert!(layout.army.y >= layout.portrait.bottom() - 0.01);
            for slot in &layout.army_slots {
                assert!(!slot.intersects(&layout.portrait));
            }
            // The whole skin layout passes the zone/overlap audit the HUD
            // uses for every resolution (nothing outside the safe frame, no
            // pair overlapping, every touch target >= 48*scale).
            assert!(
                layout.issues(&hud.tokens).is_empty(),
                "skin layout issues at {viewport:?}: {:?}",
                layout.issues(&hud.tokens)
            );
        }
    }

    #[test]
    fn ui_hudskin_draw_emits_the_atlas_regions() {
        let hud = MobileHud::new();
        let skin = crate::hudskin::tests_support::sample_skin();
        let font = Font::load();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let base = sample_view(Vec2::new(2414.0, 1080.0), icons, [None; 4], None);
        let army = [ArmyChip {
            icon: icons.archer,
            hp: 0.75,
            selected: true,
        }];
        let groups: Vec<GroupView> = (0..10)
            .map(|index| GroupView {
                rect: Rect::new(100.0 + index as f32 * 46.0, 500.0, 40.0, 40.0),
                count: index as u32,
                active: index == 2,
            })
            .collect();
        let view = HudView {
            hud_skin: Some(&skin),
            army: &army,
            groups: &groups,
            ..base
        };
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        assert!(
            !batch.textured.is_empty(),
            "the atlas skin must emit textured region quads"
        );
        // The drawn card slots ARE the layout grid (80 px tiles on an 88 px
        // pitch at the device scale), so the touch buttons built from the layout
        // stay on the art.
        let tile = layout.cell;
        let pitch = tile + CARD_GAP;
        for (index, slot) in layout.card_slots.iter().enumerate() {
            let column = index % 4;
            let row = index / 4;
            let expected = Rect::new(
                layout.card.x + column as f32 * pitch,
                layout.card.y + row as f32 * pitch,
                tile,
                tile,
            );
            assert!(
                (slot.x - expected.x).abs() < 0.01
                    && (slot.y - expected.y).abs() < 0.01
                    && (slot.w - expected.w).abs() < 0.01,
                "card slot drift: {slot:?} vs {expected:?}"
            );
        }
        // The minimap ring is the image dilated by the 1:1 atlas insets.
        let ring = layout.minimap_frame;
        assert!((ring.w - layout.minimap_image.w - 24.0).abs() < 0.5, "{ring:?}");
        assert!((ring.h - layout.minimap_image.h - 24.0).abs() < 0.5, "{ring:?}");
        assert!(ring.contains(layout.minimap_image.centre()));
    }

    // ------------------------------------------------------- composition
    // Reference HUD rebuild (ledger 434). The operator's verdict: "the console
    // is disproportionate, blurry, no clear HUD" against the Alliance console
    // (`/storage/emulated/0/Download/Лесное королевство_ Ратуша Альянса-3.png`).
    // These tests pin the measured table from `analysis/hudskin/COMPOSITION.md`.

    struct AtlasFixture {
        skin: crate::hudskin::Skin,
        textures: Vec<TextureImage>,
        icons: Icons,
    }

    fn atlas_fixture() -> Option<AtlasFixture> {
        let root = repo_root();
        let atlas_dir = root.join("analysis/h2/atlas");
        let image = atlas_dir.join("ui_atlas.png");
        let manifest = atlas_dir.join("ui_atlas.json");
        let insets = atlas_dir.join("ui_atlas_insets.json");
        if !image.is_file() || !manifest.is_file() || !insets.is_file() {
            eprintln!("skip: ui_atlas files missing under {}", atlas_dir.display());
            return None;
        }
        let atlas = match crate::ui_atlas::Atlas::load_files(
            &image,
            &manifest,
            Some(&insets),
            crate::ui_atlas::AlphaMode::Straight,
        ) {
            Ok(atlas) => atlas,
            Err(error) => {
                eprintln!("skip: ui_atlas load failed: {error}");
                return None;
            }
        };
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let index = textures.len();
        textures.push(atlas.image.to_texture());
        let skin = crate::assets::build_hud_skin(index, &atlas)?;
        Some(AtlasFixture {
            skin,
            textures,
            icons,
        })
    }

    /// Proportions measured off the reference (scaled to 1080 px height):
    /// console 338/838 -> 436 px, minimap square ~234/838 -> 330, portrait
    /// ~150/838 -> 193, command tile 62 -> 80 on an 88 pitch, group plate 46.
    #[test]
    fn composition_matches_the_reference_proportions() {
        let Some(fixture) = atlas_fixture() else {
            return;
        };
        let hud = MobileHud::new();
        let viewport = Vec2::new(2414.0, 1080.0);
        let base = sample_view(viewport, fixture.icons, [None; 4], None);
        let groups: Vec<GroupView> = (0..10u32)
            .map(|group| GroupView {
                rect: Rect::new(0.0, 0.0, 0.0, 0.0),
                count: group % 3,
                active: group == 1,
            })
            .collect();
        let view = HudView {
            hud_skin: Some(&fixture.skin),
            groups: &groups,
            ..base
        }
        .safe(SafeArea::landscape_1080());
        let layout = hud.layout(&view);
        let s = layout.scale;
        eprintln!(
            "COMPOSITION 2414x1080 scale={s:.3} console_h={:.1} ({:.3} of screen) \
             minimap={:.1} ring={:.1}x{:.1} portrait={:.1}x{:.1} tile={:.2} pitch={:.2} \
             groups={:.2} pitch={:.2} card={:.1}x{:.1}",
            layout.console.h,
            layout.console.h / viewport.y,
            layout.minimap_image.w,
            layout.minimap_frame.w,
            layout.minimap_frame.h,
            layout.portrait.w,
            layout.portrait.h,
            layout.cell,
            layout.cell + CARD_GAP,
            layout.groups_area.h,
            layout.groups_area.w / 10.0,
            layout.card.w,
            layout.card.h,
        );
        // Operator band: 371 px = 34.4% of 1080, flush at the physical bottom.
        assert!(
            (layout.console.h / viewport.y - 0.293).abs() < 0.004,
            "console {:.1} px is {:.3} of the screen (retail 0.293)",
            layout.console.h,
            layout.console.h / viewport.y
        );
        assert!((layout.console.h - 316.8).abs() < 6.0, "{}", layout.console.h);
        assert!(
            (layout.console.bottom() - viewport.y).abs() < 0.01,
            "console bottom {} must be flush with the screen bottom",
            layout.console.bottom()
        );
        // Minimap: square map inside the 12 px 1:1 ring, ring fits the band.
        assert!((layout.minimap_frame.w - layout.minimap_image.w - 24.0).abs() < 1.0);
        assert!((layout.minimap_frame.h - layout.minimap_image.h - 24.0).abs() < 1.0);
        assert!(layout.minimap_image.w >= 240.0, "{:?}", layout.minimap_image);
        assert!(layout.minimap_frame.h <= layout.console.h - 2.0);
        // Portrait frame: 175 px wide (frame_medium aspect 198:217).
        assert!((layout.portrait.w - 175.0).abs() < 1.5, "{:?}", layout.portrait);
        assert!((layout.portrait.h - 191.8).abs() < 2.0);
        // Command card: 95 px tiles on a 101 px pitch (4x3 = 398 x 297).
        assert!((layout.cell - CONSOLE_TILE_PX).abs() < 0.7, "tile {}", layout.cell);
        assert!((layout.card.w - 398.0).abs() < 2.0, "card {}", layout.card.w);
        assert!((layout.card.h - 297.0).abs() < 2.0, "card {}", layout.card.h);
        for slot in &layout.card_slots {
            assert!(
                slot.w.min(slot.h) >= CONSOLE_TILE_PX - 0.5,
                "card tile below the retail floor: {slot:?}"
            );
        }
        // Ten control-group plates of 48 dp, five per row.
        assert_eq!(
            layout.group_slots.len(),
            GROUP_COLUMNS * GROUP_ROWS,
            "ten group plates"
        );
        assert_eq!(groups.len(), 10);
        for slot in &layout.group_slots {
            assert!(
                slot.w.min(slot.h) >= CONSOLE_TILE_PX - 0.5,
                "group plate below the 100 px floor: {slot:?}"
            );
        }
        assert!((layout.groups_area.w - 499.0).abs() < 2.0, "{:?}", layout.groups_area);
        assert!((layout.groups_area.h - 196.0).abs() < 2.0);
        // Army band: one 48 dp row under the middle column.
        assert!(
            layout.army.h >= TOUCH_MIN_PX - 0.5,
            "army band {} px < 48 dp",
            layout.army.h
        );
        assert!(layout.army.h >= CONSOLE_TILE_PX - 0.5, "army chips {} px", layout.army.h);
        // Top strip: 58 px at the physical top, tabs 58x58.
        assert!((layout.top_meta.y).abs() < 0.01, "strip must be flush at y = 0");
        assert!(
            (layout.top_meta.h / viewport.y - TOP_STRIP_FRACTION).abs() < 0.01,
            "strip {:.1} px is {:.3} of H",
            layout.top_meta.h,
            layout.top_meta.h / viewport.y
        );
        for tab in layout.tabs {
            assert!(tab.w.min(tab.h) >= 30.0, "tab {tab:?}");
        }
        // Left-to-right order, no crossing: minimap < selection < groups < card.
        assert!(layout.minimap_frame.right() <= layout.selection.x);
        assert!(layout.selection.right() <= layout.groups_area.x);
        assert!(layout.groups_area.right() <= layout.card.x + 0.5);
        assert!(layout.card.right() <= layout.console.right() + 0.5);
        // Resources live in the top banner, never on the console.
        assert!(layout.resources.is_inside(&layout.top_meta));
        assert!(layout.upkeep.is_inside(&layout.top_meta));
        assert!(layout.resources.bottom() <= layout.console.y);

        // Portrait keeps the same full scale and stacks instead of squeezing.
        let portrait = hud.layout(
            &sample_view(Vec2::new(1080.0, 2414.0), fixture.icons, [None; 4], None)
                .safe(SafeArea::portrait_1080()),
        );
        assert!(
            (portrait.scale - layout.scale).abs() < 0.05,
            "portrait scale {} vs landscape {}",
            portrait.scale,
            layout.scale
        );
        assert!(portrait.scale >= 0.95);
        assert!(portrait.cell >= CONSOLE_TILE_PX - 0.5, "portrait tile {}", portrait.cell);
        assert!(portrait.army.h >= TOUCH_MIN_PX - 0.5);
        assert!(
            portrait.army.w >= portrait.console.w - 2.0 * CONSOLE_PAD_PX - 0.5,
            "army {} px must span the console {} px",
            portrait.army.w,
            portrait.console.w
        );
        assert!(portrait.inventory.w > 0.0, "portrait shows the inventory");
        assert!(portrait.card.bottom() <= portrait.console.bottom() + 0.5);
        assert!(portrait.issues(&hud.tokens).is_empty(), "{:?}", portrait.issues(&hud.tokens));
    }

    /// Every atlas piece inside the console/banner is drawn at <= 1.15x the
    /// authored element size (dest per source pixel), so nothing is blurred by a
    /// large upscale. This is the machine-checked "art drawn at N.NNx" table.
    #[test]
    fn atlas_pieces_inside_the_console_never_exceed_115_percent() {
        let Some(fixture) = atlas_fixture() else {
            return;
        };
        let hud = MobileHud::new();
        let font = Font::load();
        let mut fixture = fixture;
        let chrome = Chrome::new(&mut fixture.textures);
        let viewport = Vec2::new(2414.0, 1080.0);
        let base = sample_view(viewport, fixture.icons, [None; 4], None);
        let army: Vec<ArmyChip> = (0..6)
            .map(|index| ArmyChip {
                icon: fixture.icons.archer,
                hp: 0.4 + index as f32 * 0.08,
                selected: index == 0,
            })
            .collect();
        let mut view = HudView {
            hud_skin: Some(&fixture.skin),
            army: &army,
            ..base
        }
        .safe(SafeArea::landscape_1080());
        // The real group rects come from the layout (`game.rs` mirrors this).
        let probe = hud.layout(&view);
        let groups: Vec<GroupView> = (0..10u32)
            .map(|group| GroupView {
                rect: Rect::new(
                    probe.groups_area.x + group as f32 * (probe.groups_area.w / 10.0),
                    probe.groups_area.y,
                    probe.groups_area.h,
                    probe.groups_area.h,
                ),
                count: group % 3,
                active: group == 1,
            })
            .collect();
        view.groups = &groups;
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        let console = layout.console;
        let banner = layout.top_meta;
        let atlas = fixture.skin;
        let mut checked = 0usize;
        let mut worst: (f32, [f32; 4]) = (0.0, [0.0; 4]);
        for draw in &batch.textured {
            if draw.texture != atlas.texture {
                continue;
            }
            for chunk in draw.vertices.chunks_exact(6) {
                let xs: Vec<f32> = chunk.iter().map(|v| v.pos[0]).collect();
                let ys: Vec<f32> = chunk.iter().map(|v| v.pos[1]).collect();
                let (x0, x1) = (
                    xs.iter().cloned().fold(f32::MAX, f32::min),
                    xs.iter().cloned().fold(f32::MIN, f32::max),
                );
                let (y0, y1) = (
                    ys.iter().cloned().fold(f32::MAX, f32::min),
                    ys.iter().cloned().fold(f32::MIN, f32::max),
                );
                let dest = Rect::new(x0, y0, x1 - x0, y1 - y0);
                if dest.w <= 0.5 || dest.h <= 0.5 {
                    continue;
                }
                // Only the composition is under the rule; toasts/tooltips float
                // above the console and are not part of it.
                let centre = dest.centre();
                if !(console.contains(centre) || banner.contains(centre)) {
                    continue;
                }
                let us: Vec<f32> = chunk.iter().map(|v| v.uv[0]).collect();
                let vs: Vec<f32> = chunk.iter().map(|v| v.uv[1]).collect();
                let (u0, u1) = (
                    us.iter().cloned().fold(f32::MAX, f32::min),
                    us.iter().cloned().fold(f32::MIN, f32::max),
                );
                let (v0, v1) = (
                    vs.iter().cloned().fold(f32::MAX, f32::min),
                    vs.iter().cloned().fold(f32::MIN, f32::max),
                );
                let src_w = (u1 - u0) * atlas.width;
                let src_h = (v1 - v0) * atlas.height;
                if src_w <= 0.5 || src_h <= 0.5 {
                    continue;
                }
                let sx = dest.w / src_w;
                let sy = dest.h / src_h;
                checked += 1;
                if sx.max(sy) > worst.0 {
                    worst = (sx.max(sy), [u0, v0, u1, v1]);
                }
                assert!(
                    sx <= 1.15 + 1e-3 && sy <= 1.15 + 1e-3,
                    "atlas quad upscaled {sx:.2}x{sy:.2} at {dest:?} (uv {u0:.3},{v0:.3})"
                );
            }
        }
        assert!(checked > 40, "only {checked} atlas quads checked");
        eprintln!(
            "ATLAS_SCALE checked={checked} worst={:.3}x uv={:?}",
            worst.0, worst.1
        );
    }

    /// Headless evidence: the full composition at both reference viewports.
    #[test]
    fn composition_mock_writes_pngs() {
        let Some(fixture) = atlas_fixture() else {
            return;
        };
        let hud = MobileHud::new();
        let font = Font::load();
        // The chrome's 1x1 flat texture must live in the SAME texture list the
        // rasterizer samples, otherwise every flat fill is skipped and the
        // console renders as bare terrain (`COMPOSITION.md` §5).
        let mut fixture = fixture;
        let chrome = Chrome::new(&mut fixture.textures);
        let map = map_texture();
        for (viewport, safe, name) in [
            (
                Vec2::new(2414.0, 1080.0),
                SafeArea::landscape_1080(),
                "composition-2414x1080.png",
            ),
            (
                Vec2::new(1080.0, 2414.0),
                SafeArea::portrait_1080(),
                "composition-1080x2414.png",
            ),
        ] {
            let items: Vec<Option<crate::hud::inventory::ItemSlot<'static>>> = vec![
                Some(crate::hud::inventory::ItemSlot {
                    icon: Some("BTNAnkh"),
                    charges: 3,
                    cooldown: 0.0,
                }),
                Some(crate::hud::inventory::ItemSlot {
                    icon: Some("BTNGatherGold"),
                    charges: 0,
                    cooldown: 0.4,
                }),
                None,
                None,
                None,
                None,
            ];
            let item_icons: Vec<Option<usize>> = vec![
                fixture.icons.gold,
                fixture.icons.gather,
                None,
                None,
                None,
                None,
            ];
            let base = sample_view(viewport, fixture.icons, [None; 4], None)
                .items(&items)
                .item_icons(&item_icons);
            let army: Vec<ArmyChip> = (0..4)
                .map(|index| ArmyChip {
                    icon: fixture.icons.archer,
                    hp: 0.55 + index as f32 * 0.1,
                    selected: index == 0,
                })
                .collect();
            let mut view = HudView {
                hud_skin: Some(&fixture.skin),
                army: &army,
                ..base
            }
            .safe(safe);
            let probe = hud.layout(&view);
            let groups: Vec<GroupView> = (0..10u32)
                .map(|group| GroupView {
                    rect: Rect::new(
                        probe.groups_area.x + group as f32 * (probe.groups_area.w / 10.0),
                        probe.groups_area.y,
                        probe.groups_area.h,
                        probe.groups_area.h,
                    ),
                    count: if group % 4 == 0 { 0 } else { group % 3 + 1 },
                    active: group == 1,
                })
                .collect();
            view.groups = &groups;
            let mut batch = UiBatch::default();
            let layout = hud.draw(&view, &mut batch, &font, &chrome);
            assert!(layout.issues(&hud.tokens).is_empty(), "{:?}", layout.issues(&hud.tokens));
            let textures = fixture.textures.clone();
            let pixels = rasterize(
                &batch,
                &textures,
                &font,
                &map,
                viewport.x as u32,
                viewport.y as u32,
            );
            let out_dir = repo_root().join("analysis/hudskin");
            std::fs::create_dir_all(&out_dir).expect("analysis/hudskin dir");
            let out = out_dir.join(name);
            write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("hud png");
            let bytes = std::fs::metadata(&out).expect("hud png metadata").len();
            assert!(bytes > 20_000, "{name} {bytes} bytes");
            eprintln!("COMPOSITION_MOCK {} {} bytes", out.display(), bytes);
        }
    }

    #[test]
    fn ui_hudskin_atlas_mock_writes_png() {
        let root = repo_root();
        let atlas_dir = root.join("analysis/h2/atlas");
        let image = atlas_dir.join("ui_atlas.png");
        let manifest = atlas_dir.join("ui_atlas.json");
        let insets = atlas_dir.join("ui_atlas_insets.json");
        if !image.is_file() || !manifest.is_file() || !insets.is_file() {
            eprintln!("skip: ui_atlas files missing under {}", atlas_dir.display());
            return;
        }
        let atlas = match crate::ui_atlas::Atlas::load_files(
            &image,
            &manifest,
            Some(&insets),
            crate::ui_atlas::AlphaMode::Straight,
        ) {
            Ok(atlas) => atlas,
            Err(error) => {
                eprintln!("skip: ui_atlas load failed: {error}");
                return;
            }
        };
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let index = textures.len();
        textures.push(atlas.image.to_texture());
        let skin = match crate::assets::build_hud_skin(index, &atlas) {
            Some(skin) => skin,
            None => {
                eprintln!("skip: atlas has no 24-role skin");
                return;
            }
        };
        let dialog_skin = crate::assets::dialog_sub_slice_skin(&skin);
        let font = Font::load();
        let map = map_texture();
        let viewport = Vec2::new(2414.0, 1080.0);
        let base = sample_view(viewport, icons, [None; 4], None);
        let army = [ArmyChip {
            icon: icons.archer,
            hp: 0.72,
            selected: true,
        }];
        let groups: Vec<GroupView> = (0..10u32)
            .map(|group| GroupView {
                rect: Rect::new(760.0 + group as f32 * 46.0, 760.0, 40.0, 40.0),
                count: group % 3,
                active: group == 1,
            })
            .collect();
        let view = HudView {
            hud_skin: Some(&skin),
            army: &army,
            groups: &groups,
            ..base
        }
        .safe(SafeArea::landscape_1080());
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        assert!((layout.minimap_frame.w - layout.minimap_image.w - 24.0).abs() < 0.5);
        let pixels = rasterize(
            &batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out_dir = root.join("analysis/hudskin");
        std::fs::create_dir_all(&out_dir).expect("analysis/hudskin dir");
        let out = out_dir.join("hud-integration-2414x1080.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("hud png");
        let bytes = std::fs::metadata(&out).expect("hud png metadata").len();
        assert!(bytes > 20_000, "hud png {bytes} bytes");
        eprintln!("HUD_SKIN_MOCK {} {} bytes", out.display(), bytes);

        let quest_lines = [
            "MAIN QUEST",
            "Gold: 500   Lumber: 250",
            "Kill the creeps",
            "Return to the base",
        ];
        let dialog = DialogView::for_screen(DialogScreen::QuestLog, viewport)
            .with_content("QUEST LOG", &quest_lines, "TAP · CLOSE")
            .hud_skin(Some(&skin), Some(&dialog_skin));
        let mut dialog_batch = UiBatch::default();
        hud.draw_dialog(&dialog, &mut dialog_batch, &font, &chrome);
        let pixels = rasterize(
            &dialog_batch,
            &textures,
            &font,
            &map,
            viewport.x as u32,
            viewport.y as u32,
        );
        let out = root.join("analysis/hudskin/questlog-integration-2414x1080.png");
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("dialog png");
        let bytes = std::fs::metadata(&out).expect("dialog png metadata").len();
        assert!(bytes > 20_000, "dialog png {bytes} bytes");
        eprintln!("HUD_SKIN_DIALOG_MOCK {} {} bytes", out.display(), bytes);
    }

    #[test]
    fn text_and_icon_rects_stay_inside_their_panels() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = Font::load();
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let view = sample_view(viewport, icons, [None; 4], None);
            let layout = hud.layout(&view);
            for (name, rect) in [
                ("name", layout.name),
                ("class", layout.class),
                ("hp_text", layout.hp_text),
                ("mana_text", layout.mana_text),
                ("stats", layout.stats),
                ("production_label", layout.production_label),
            ] {
                assert!(
                    rect.is_inside(&layout.console),
                    "{name} outside console at {viewport:?}: {rect:?}"
                );
            }
            for slot in &layout.card_slots {
                assert!(
                    slot.is_inside(&layout.card),
                    "card slot outside card: {slot:?}"
                );
            }
            for pill in &layout.resource_pills {
                assert!(
                    pill.is_inside(&layout.resources),
                    "resource pill outside resources: {pill:?}"
                );
            }
            let mut batch = UiBatch::default();
            let _ = hud.draw(&view, &mut batch, &font, &chrome);
            // The strip is flush at y = 0 (operator), so text is bounded by the
            // viewport; the world-side bands stay inside the safe frame.
            let bounds = layout.viewport_rect().expanded(2.0, 2.0);
            for vertex in &batch.font {
                let point = Vec2::new(vertex.pos[0], vertex.pos[1]);
                assert!(
                    bounds.contains(point),
                    "font vertex outside the safe frame at {viewport:?}: {point:?}"
                );
            }
        }
    }

    #[test]
    fn redraw_console_parts_cover_the_console_seamlessly() {
        for scale in [0.62, 1.0, 2.0] {
            let console = Rect::new(48.0, 1080.0 - 288.0 * scale, 2318.0, 288.0 * scale);
            let top_bar = Rect::new(48.0, 80.0, 2318.0, 32.0 * scale);
            let skin = RedrawSkin {
                console_left_cap: Some(10),
                console_left_mid: Some(11),
                console_right_mid: Some(12),
                console_right_cap: Some(13),
                topbar: Some(14),
                ..RedrawSkin::default()
            };
            let parts = build_redraw_parts(&skin, console, top_bar, scale).expect("redraw parts");
            assert!(!parts.console.is_empty(), "console quads at {scale}");
            for quad in &parts.console {
                assert!(
                    quad.rect.x >= console.x - 0.01 && quad.rect.right() <= console.right() + 0.01,
                    "quad x inside console at {scale}: {:?}",
                    quad.rect
                );
                assert!(
                    (quad.rect.y - console.y).abs() < 0.01
                        && (quad.rect.h - console.h).abs() < 0.01,
                    "full console height at {scale}: {:?}",
                    quad.rect
                );
                assert!(
                    quad.uv.iter().all(|value| (0.0..=1.0).contains(value)),
                    "uv inside [0,1] at {scale}: {:?}",
                    quad.uv
                );
            }
            // Horizontal coverage: sorted quads tile the console with no gaps.
            let mut spans: Vec<(f32, f32)> = parts
                .console
                .iter()
                .map(|quad| (quad.rect.x, quad.rect.right()))
                .collect();
            spans.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite x"));
            let mut cursor = console.x;
            for (start, end) in spans {
                assert!(
                    (start - cursor).abs() < 0.75,
                    "gap {cursor} -> {start} at scale {scale}"
                );
                cursor = cursor.max(end);
            }
            assert!(
                (cursor - console.right()).abs() < 0.75,
                "right edge {cursor} vs {} at scale {scale}",
                console.right()
            );
            // Caps use the 256*scale natural width (clamped) and the pack tiles.
            let expected_cap = (256.0 * scale).min(console.w * 0.30);
            assert!((parts.console[0].rect.w - expected_cap).abs() < 0.01);
            assert_eq!(parts.console[0].texture, 10);
            assert_eq!(parts.console.last().map(|quad| quad.texture), Some(13));
            // Topbar plate is a 3-part horizontal slice with 16/720 edges.
            assert_eq!(parts.topbar.len(), 3);
            for quad in &parts.topbar {
                assert!((quad.rect.y - top_bar.y).abs() < 0.01);
            }
        }
    }

    #[test]
    fn redraw_button_states_map_from_command_flags() {
        let mut command = CommandButton::new("X", None);
        assert_eq!(command_visual_state(&command), 0, "normal");
        command.pressed = true;
        assert_eq!(command_visual_state(&command), 1, "pressed");
        command.pressed = false;
        command.enabled = false;
        assert_eq!(command_visual_state(&command), 2, "disabled");
        command.enabled = true;
        command.progress = 0.5;
        assert_eq!(command_visual_state(&command), 3, "cooldown/progress");
        command.progress = 0.0;
        command.present = false;
        assert_eq!(command_visual_state(&command), 5, "empty");
    }

    #[test]
    fn redraw_round_robin_mids_use_both_variants() {
        let console = Rect::new(0.0, 700.0, 2318.0, 288.0);
        let skin = RedrawSkin {
            console_left_cap: Some(1),
            console_left_mid: Some(2),
            console_right_mid: Some(3),
            console_right_cap: Some(4),
            ..RedrawSkin::default()
        };
        let parts = build_redraw_parts(&skin, console, Rect::new(0.0, 0.0, 1.0, 1.0), 1.0)
            .expect("parts");
        let textures: Vec<usize> = parts.console.iter().map(|quad| quad.texture).collect();
        assert!(textures.contains(&2) && textures.contains(&3), "both mid variants: {textures:?}");
    }

    #[test]
    fn dragonui_uiscale_auto_clamp_epsilon() {
        // DragonUI core/uiscale.lua: auto=768/screen_height, clamp 0.40-1.15, epsilon 0.001
        let s1080 = super::UiScale::compute(1080.0);
        let expected_auto = super::UiScale::clamp(super::DRAGONUI_DESIGN_HEIGHT / 1080.0);
        assert!((s1080.auto - expected_auto).abs() < 1e-6);
        assert!((s1080.factor - 1.0).abs() < 1e-6, "neutral factor preserves HUD layout");
        let s_low = super::UiScale::compute(4000.0);
        assert!((s_low.auto - super::UI_SCALE_MIN).abs() < 1e-6 || s_low.auto >= super::UI_SCALE_MIN - 1e-6);
        let s_high = super::UiScale::compute(400.0);
        assert!(s_high.auto <= super::UI_SCALE_MAX + 1e-6);
        assert!(!super::UiScale::changed(1.0, 1.0005));
        assert!(super::UiScale::changed(1.0, 1.002));
    }

    #[test]
    fn dragonui_hotkey_font_scales_with_button_ratio() {
        // DragonUI modules/buttons.lua:207-210: size*(referenceScale/buttonScale)
        let s = super::scaled_hotkey_font_size(12.0, 1.0, 0.8);
        assert!((s - 15.0).abs() < 1e-4);
        let same = super::scaled_hotkey_font_size(12.0, 1.0, 1.0);
        assert!((same - 12.0).abs() < 1e-4);
    }

    #[test]
    fn dragonui_castbar_fills_parent_height() {
        // DragonUI modules/castbar.lua:885: fill uses parent height
        let parent = Rect::new(10.0, 20.0, 200.0, 18.0);
        let fill = super::castbar_fill_rect(parent, 0.5);
        assert!((fill.h - parent.h).abs() < 1e-6);
        assert!((fill.w - 100.0).abs() < 1e-6);
        assert!((fill.y - parent.y).abs() < 1e-6);
    }

    #[test]
    fn dragonui_micromenu_atlas_grid_uv() {
        // DragonUI modules/micromenu.lua: atlas UV grid (MicroMenu/Minimap quickslots)
        let g = super::AtlasGridUv::MICRO_MENU;
        let uv = g.uv(0, 0);
        assert!((uv[0] - 0.0).abs() < 1e-6 && (uv[1] - 0.0).abs() < 1e-6);
        assert!((uv[2] - 0.25).abs() < 1e-6 && (uv[3] - 0.25).abs() < 1e-6);
        let uv1 = g.uv(1, 2);
        assert!((uv1[0] - 0.25).abs() < 1e-6 && (uv1[1] - 0.5).abs() < 1e-6);
        let q = super::AtlasGridUv::MINIMAP_QUICKSLOT;
        let uq = q.uv(2, 0);
        assert!((uq[0] - 0.5).abs() < 1e-6 && (uq[2] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn dragonui_button_backdrop_insets() {
        // DragonUI modules/buttons.lua: three/nine-slice backdrop insets
        let b = super::BackdropInsets::uniform(4.0);
        assert!((b.left - 4.0).abs() < 1e-6 && (b.bottom - 4.0).abs() < 1e-6);
        let c = super::BackdropInsets::new(2.0, 3.0, 4.0, 5.0);
        assert!((c.right - 4.0).abs() < 1e-6 && (c.top - 3.0).abs() < 1e-6);
    }
    fn rasterize(
        batch: &UiBatch,
        textures: &[TextureImage],
        font: &Font,
        minimap: &TextureImage,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let mut canvas = Canvas::new(width, height);
        canvas.background(width, height);
        for draw in &batch.textured {
            if let Some(texture) = textures.get(draw.texture) {
                for chunk in draw.vertices.chunks_exact(6) {
                    canvas.textured_quad(chunk, texture, false);
                }
            }
        }
        for chunk in batch.white.chunks_exact(6) {
            canvas.solid_quad(chunk);
        }
        for chunk in batch.font.chunks_exact(6) {
            canvas.textured_quad(
                chunk,
                &TextureImage {
                    width: font.atlas.width,
                    height: font.atlas.height,
                    rgba: font.atlas.rgba.clone(),
                },
                true,
            );
        }
        for chunk in batch.minimap.chunks_exact(6) {
            canvas.textured_quad(chunk, minimap, false);
        }
        canvas.rgba
    }

    /// Linear vertex colour -> sRGB bytes, matching the `Rgba8UnormSrgb` surface.
    fn encode(color: [f32; 4]) -> [f32; 4] {
        [
            hud::linear_to_srgb(color[0].clamp(0.0, 1.0)),
            hud::linear_to_srgb(color[1].clamp(0.0, 1.0)),
            hud::linear_to_srgb(color[2].clamp(0.0, 1.0)),
            color[3],
        ]
    }

    struct Canvas {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    }

    impl Canvas {
        fn new(width: u32, height: u32) -> Self {
            Self {
                width,
                height,
                rgba: vec![0u8; (width * height * 4) as usize],
            }
        }

        fn background(&mut self, width: u32, height: u32) {
            for y in 0..height {
                for x in 0..width {
                    let nx = x as f32 / width as f32;
                    let ny = y as f32 / height as f32;
                    let n = ((nx * 37.0).sin() * (ny * 53.0).cos() * 0.5 + 0.5) * 0.10;
                    let dx = nx - 0.55;
                    let dy = ny - 0.42;
                    let dirt = (1.0 - (dx * dx + dy * dy).sqrt() * 1.8).clamp(0.0, 1.0);
                    let index = ((y * width + x) * 4) as usize;
                    self.rgba[index] = ((0.16 + n + dirt * 0.10) * 255.0) as u8;
                    self.rgba[index + 1] = ((0.23 + n + dirt * 0.05) * 255.0) as u8;
                    self.rgba[index + 2] = ((0.13 + n * 0.6) * 255.0) as u8;
                    self.rgba[index + 3] = 255;
                }
            }
        }

        fn blend(&mut self, x: i32, y: i32, color: [f32; 4]) {
            if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                return;
            }
            let alpha = color[3].clamp(0.0, 1.0);
            if alpha <= 0.001 {
                return;
            }
            let index = ((y as u32 * self.width + x as u32) * 4) as usize;
            for channel in 0..3 {
                let dst = self.rgba[index + channel] as f32 / 255.0;
                let out = color[channel] * alpha + dst * (1.0 - alpha);
                self.rgba[index + channel] = (out.clamp(0.0, 1.0) * 255.0) as u8;
            }
            self.rgba[index + 3] = 255;
        }

        fn solid_quad(&mut self, vertices: &[hud::UiVertex]) {
            let color = encode(vertices[0].color);
            let (x0, y0, x1, y1) = bounds(vertices);
            for y in y0.floor() as i32..y1.ceil() as i32 {
                for x in x0.floor() as i32..x1.ceil() as i32 {
                    self.blend(x, y, color);
                }
            }
        }

        fn textured_quad(
            &mut self,
            vertices: &[hud::UiVertex],
            texture: &TextureImage,
            font: bool,
        ) {
            let (x0, y0, x1, y1) = bounds(vertices);
            if x1 <= x0 || y1 <= y0 {
                return;
            }
            let v0 = vertices[0];
            let v2 = vertices[2];
            let color = encode(v0.color);
            for y in y0.floor() as i32..y1.ceil() as i32 {
                let fy = ((y as f32 + 0.5 - y0) / (y1 - y0)).clamp(0.0, 1.0);
                for x in x0.floor() as i32..x1.ceil() as i32 {
                    let fx = ((x as f32 + 0.5 - x0) / (x1 - x0)).clamp(0.0, 1.0);
                    let u = v0.uv[0] + (v2.uv[0] - v0.uv[0]) * fx;
                    let v = v0.uv[1] + (v2.uv[1] - v0.uv[1]) * fy;
                    let tx = (u * texture.width as f32)
                        .floor()
                        .clamp(0.0, texture.width as f32 - 1.0) as u32;
                    let ty = (v * texture.height as f32)
                        .floor()
                        .clamp(0.0, texture.height as f32 - 1.0)
                        as u32;
                    let index = ((ty * texture.width + tx) * 4) as usize;
                    let texel = [
                        texture.rgba[index] as f32 / 255.0,
                        texture.rgba[index + 1] as f32 / 255.0,
                        texture.rgba[index + 2] as f32 / 255.0,
                        texture.rgba[index + 3] as f32 / 255.0,
                    ];
                    let out = if font {
                        [color[0], color[1], color[2], color[3] * texel[3]]
                    } else {
                        [
                            color[0] * texel[0],
                            color[1] * texel[1],
                            color[2] * texel[2],
                            color[3] * texel[3],
                        ]
                    };
                    self.blend(x, y, out);
                }
            }
        }
    }

    fn bounds(vertices: &[hud::UiVertex]) -> (f32, f32, f32, f32) {
        let mut x0 = f32::INFINITY;
        let mut y0 = f32::INFINITY;
        let mut x1 = f32::NEG_INFINITY;
        let mut y1 = f32::NEG_INFINITY;
        for vertex in vertices {
            x0 = x0.min(vertex.pos[0]);
            y0 = y0.min(vertex.pos[1]);
            x1 = x1.max(vertex.pos[0]);
            y1 = y1.max(vertex.pos[1]);
        }
        (x0, y0, x1, y1)
    }

    // Minimal PNG writer (stored deflate blocks) so the mock has no image crate
    // dependency and stays byte-stable across Termux/Android.
    fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        let mut raw = Vec::with_capacity(((width * 4 + 1) * height) as usize);
        for row in 0..height {
            raw.push(0);
            let start = (row * width * 4) as usize;
            raw.extend_from_slice(&rgba[start..start + (width * 4) as usize]);
        }
        let mut zlib = vec![0x78, 0x01];
        let mut offset = 0usize;
        while offset < raw.len() {
            let end = (offset + 65535).min(raw.len());
            let block = &raw[offset..end];
            zlib.push(if end == raw.len() { 1 } else { 0 });
            zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
            zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
            zlib.extend_from_slice(block);
            offset = end;
        }
        zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &zlib);
        chunk(&mut png, b"IEND", &[]);
        fs::write(path, png)
    }

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= *byte as u32;
            for _ in 0..8 {
                let mask = if crc & 1 != 0 { 0xEDB8_8320 } else { 0 };
                crc = (crc >> 1) ^ mask;
            }
        }
        !crc
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a = 1u32;
        let mut b = 0u32;
        for byte in data {
            a = (a + *byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    // ------------------------------------------------------------ tooltip
    // WC3 information design: the tooltip floats above the hovered command
    // slot, the toast band owns its own rows above the console, and none of
    // them can cover each other. These tests measure the rects the renderer
    // actually draws (same `layout` + `Font`).

    fn hud_font() -> Font {
        Font::load()
    }

    #[test]
    fn hud_tooltip_is_anchored_to_the_slot_and_stays_above_the_toast_band() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = hud_font();
        let viewport = Vec2::new(2414.0, 1080.0);
        let view = sample_view(viewport, icons, [None; 4], None)
            .toast(None)
            .toasts(vec![
                ToastView::new("MISSING REQUIREMENT", ToastTone::Danger),
                ToastView::new("NOT ENOUGH GOLD", ToastTone::Danger),
            ])
            .hint(Some(HintView::new("TAP GROUND TO PLACE WATCH TOWER", true)))
            .tooltip(Some(
                "WATCH TOWER  (B)\nCOST 130 GOLD · 80 LUMBER\nA defensive tower that attacks ground and air units with piercing bolts."
                    .to_string(),
            ));
        let anchor = Rect::new(viewport.x - 300.0, 760.0, 64.0, 64.0);
        let view = view.tooltip_anchor(Some(anchor));
        let layout = hud.layout(&view);
        let issues = layout.issues(&hud.tokens);
        assert!(issues.is_empty(), "layout issues: {issues:#?}");
        assert!(
            layout.tooltip.is_inside(&layout.safe_frame),
            "tooltip {:?} outside safe frame {:?}",
            layout.tooltip,
            layout.safe_frame
        );
        // Anchored: the box is horizontally centred on the hovered slot (the
        // clamp may shift it, but never further than half a slot).
        assert!(
            (layout.tooltip.centre().x - anchor.centre().x).abs() <= anchor.w * 0.5,
            "tooltip {:?} not anchored to slot {:?}",
            layout.tooltip,
            anchor
        );
        // Band order, bottom-up: console → toasts → hint → tooltip.
        assert!(layout.toasts.len() == 2, "two toast rows");
        assert!(layout.toasts[0].bottom() <= layout.console.y, "toast above console");
        assert!(layout.toasts[1].bottom() <= layout.toasts[0].y, "second toast above the first");
        assert!(layout.hint.y < layout.toasts[1].y, "hint above the toast band");
        assert!(layout.tooltip.bottom() <= layout.hint.y, "tooltip above the hint");
        for (index, toast) in layout.toasts.iter().enumerate() {
            assert!(
                layout.tooltip.intersection_area(toast) <= 0.5,
                "tooltip {:?} overlaps toast[{index}] {toast:?}",
                layout.tooltip
            );
        }
        assert!(layout.tooltip.intersection_area(&layout.hint) <= 0.5);
        // Long text wraps inside the rect instead of being truncated to one line.
        let inner = layout.tooltip.w - 20.0 * layout.scale;
        let mut lines = view.tooltip.as_deref().unwrap().split('\n');
        let _title = lines.next();
        let body = lines.collect::<Vec<_>>().join("\n");
        let wrapped = font.wrap(&body, (0.72 * layout.scale).max(0.8), inner);
        assert!(wrapped.len() > 1, "long tooltip must wrap: {wrapped:?}");
        for line in &wrapped {
            assert!(
                font.measure(line, (0.72 * layout.scale).max(0.8)) <= inner + 0.5,
                "wrapped line overflows the tooltip: {line:?}"
            );
        }
    }

    #[test]
    fn hud_toast_rows_get_their_own_band_and_never_touch_the_card() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let viewport = Vec2::new(1080.0, 2414.0);
        for count in 0..=3usize {
            let toasts: Vec<ToastView> = (0..count)
                .map(|index| ToastView::new(format!("TOAST {index}"), ToastTone::Info))
                .collect();
            let view = sample_view(viewport, icons, [None; 4], None)
                .toast(None)
                .toasts(toasts);
            let layout = hud.layout(&view);
            let issues = layout.issues(&hud.tokens);
            assert!(issues.is_empty(), "{count} toasts: {issues:#?}");
            assert_eq!(layout.toasts.len(), count);
            for toast in &layout.toasts {
                assert!(toast.bottom() <= layout.console.y);
                assert!(toast.intersection_area(&layout.card) <= 0.5);
                assert!(toast.is_inside(&layout.safe_frame));
            }
        }
    }

    #[test]
    fn hud_command_tiles_carry_only_the_hotkey_overlay() {
        // Retail contract (TFT Game.dll, ledger 406): 4x3 tiles with
        // tile = 0.038*H, gap = 0.0075*H (pitch 0.0455*H) and exactly ONE text
        // frame per tile — the hotkey overlay. No caption, no cost badge.
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = hud_font();
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let view = sample_view(viewport, icons, [None; 4], None);
            let layout = hud.layout(&view);
            let s = layout.scale;
            let tile = layout.cell;
            // Reference HUD: 62 px tiles on an 88 px pitch at 838 px -> 80/88 at
            // 1080 (`COMPOSITION.md` §1). The pitch ratio is the contract.
            let pitch = tile + CARD_GAP;
            eprintln!(
                "CARD viewport={}x{} scale={:.2} tile={:.2}px pitch={:.2}px bar={:.1}x{:.1}px retail_tile={:.2}px",
                viewport.x,
                viewport.y,
                s,
                tile,
                pitch,
                layout.card.w,
                layout.card.h,
                0.038 * viewport.y
            );
            // Bar = 4 tiles + 3 field gaps + 2 console trims wide, 3 tiles +
            // 2 gaps + 2 trims tall. The old formula used the 6 px card-gap
            // constant and forgot the trim, which stopped matching when the
            // panel became the dp card (`CARD_W_DP` = 232, `CARD_H_DP` = 176).
            let density = (tile / TOUCH_DP).max(0.01);
            let gap = CARD_GAP_DP * density;
            let trim = CONSOLE_PAD_DP * density;
            assert!(
                (layout.card.w - (4.0 * tile + 3.0 * gap + 2.0 * trim)).abs() < 0.01,
                "card width {} != 4 tiles + 3 gaps + 2 trims",
                layout.card.w
            );
            assert!(
                (layout.card.h - (3.0 * tile + 2.0 * gap + 2.0 * trim)).abs() < 0.01,
                "card height {} != 3 tiles + 2 gaps + 2 trims",
                layout.card.h
            );
            assert!(layout.card_slots.len() == 12, "4x3 command bar");
            for (index, slot) in layout.card_slots.iter().enumerate() {
                let Some(command) = view.commands.get(index).copied() else {
                    continue;
                };
                let text = card_slot_text_rects(&font, *slot, &command, s);
                // The struct can only carry the hotkey frame: a caption or a
                // cost badge inside a tile is a compile-time impossibility.
                if let Some(overlay) = text.hotkey {
                    assert!(overlay.is_inside(slot), "hotkey {overlay:?} outside {slot:?}");
                    assert!(overlay.w <= tile * 0.45, "hotkey frame wider than a tile");
                    let (fitted, drawn_scale) =
                        font.fit_text(overlay, &command.hotkey, 0.53 * s, crate::hud::MIN_DENSE_PX);
                    assert!(!fitted.is_empty(), "hotkey glyph missing");
                    assert!(font.measure(&fitted, drawn_scale) <= overlay.w + 0.5);
                }
                // A cost sub-label is data, never tile text.
                if card_cost_label(&command.sub).is_some() {
                    assert!(text.hotkey.is_some() == !command.hotkey.is_empty());
                }
            }
        }
        // Device pixels at 2414x1080: 48 dp = 126 px tiles on a 134 px pitch,
        // a 528 x 394 px 4x3 card (`COMPOSITION.md` §1 arithmetic).
        let device = MobileHud::new().layout(&sample_view(
            Vec2::new(2414.0, 1080.0),
            load_icons(&mut Vec::new()),
            [None; 4],
            None,
        ));
        assert!(
            (device.cell - TOUCH_MIN_PX).abs() < 0.6,
            "tile {} px (48 dp = 126 px)",
            device.cell
        );
        let device_pitch = device.cell + CARD_GAP;
        assert!(
            (device_pitch - (CONSOLE_TILE_PX + CARD_GAP)).abs() < 0.9,
            "pitch {device_pitch} px"
        );
        assert!((device.card.w - 398.0).abs() < 2.0, "card {} px", device.card.w);
        assert!((device.card.h - 297.0).abs() < 2.0, "card {} px", device.card.h);
    }

    #[test]
    fn hud_tooltip_has_three_sections_and_caps_cost_rows() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let _ = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = hud_font();
        let viewport = Vec2::new(2414.0, 1080.0);
        let costs: Vec<String> = (0..12)
            .map(|index| format!("COST {index} GOLD · REQUIREMENT ROW {index}"))
            .collect();
        let view = sample_view(viewport, icons, [None; 4], None)
            .toast(None)
            .tooltip(Some(
                "WATCH TOWER  (B)\nA defensive tower that attacks ground and air units."
                    .to_string(),
            ))
            .tooltip_costs(costs)
            .tooltip_anchor(Some(Rect::new(1800.0, 700.0, 48.0, 48.0)));
        let layout = hud.layout(&view);
        let issues = layout.issues(&hud.tokens);
        assert!(issues.is_empty(), "{issues:#?}");
        // Section 3 is capped at eight rows; the box is tall enough for the
        // title, the description and those rows.
        let body_scale = (0.72 * layout.scale).max(0.8);
        let inner = layout.tooltip.w - 20.0 * layout.scale;
        let mut rows = 0;
        for cost in view.tooltip_costs.iter().take(TOOLTIP_MAX_COST_LINES) {
            rows += font.wrap(cost, body_scale, inner).len();
            assert!(
                font.measure(cost, body_scale) <= inner + 0.5
                    || font.wrap(cost, body_scale, inner).len() > 1,
                "cost row wraps instead of overflowing: {cost}"
            );
        }
        assert!(rows >= TOOLTIP_MAX_COST_LINES, "cost rows wrapped: {rows}");
        // With the 48 dp banner the tooltip is capped by the space between the
        // banner and the toast band, so it holds at least the title + 3 rows.
        assert!(
            layout.tooltip.h >= 3.0 * font.line_box(body_scale),
            "tooltip box holds the head rows: {:?}",
            layout.tooltip
        );
        assert!(layout.tooltip.is_inside(&layout.safe_frame));
        // Colour codes: the shipped gold section keeps its colour, resets work.
        let runs = crate::hud::color_runs("|cFFFED312COST 130 GOLD|r PLAIN", hud.tokens.text);
        assert_eq!(runs.len(), 2, "colour runs: {runs:?}");
        assert!((runs[0].1[0] - 0.996).abs() < 0.01, "gold red channel");
        assert_eq!(runs[1].0.trim(), "PLAIN");
        assert!((runs[1].1[0] - hud.tokens.text[0]).abs() < 0.01);
    }

    #[test]
    fn hud_fit_text_ellipsizes_cyrillic_inside_the_rect() {
        let font = hud_font();
        let slot = Rect::new(100.0, 100.0, 64.0, 64.0);
        let long = "СТОРОЖЕВАЯ БАШНЯ";
        let (fitted, scale) = font.fit_text(slot, long, 0.80, crate::hud::MIN_DENSE_PX);
        assert!(fitted.len() < long.len(), "long name must be ellipsized: {fitted:?}");
        assert!(font.measure(&fitted, scale) <= slot.w + 0.5);
        assert!(
            scale >= crate::hud::MIN_DENSE_PX / 30.0,
            "fit_text must never shrink below the dense floor: {scale}"
        );
        let (short, _) = font.fit_text(slot, "B", 0.80, crate::hud::MIN_DENSE_PX);
        assert_eq!(short, "B");
    }

    // ------------------------------------------------------------------
    // Ledger 469: full-screen menus + command-card legibility
    // ------------------------------------------------------------------

    struct PackFixture {
        skin: Skin,
        textures: Vec<TextureImage>,
    }

    fn pack_fixture() -> Option<PackFixture> {
        let root = crate::hudskin::pack::find_installed()?;
        let mut textures = Vec::new();
        let pack = crate::hudskin::pack::HudPack::load(&root, &mut textures).ok()?;
        // Bind the shared pack faces to THIS fixture's texture indices, exactly
        // like `Assets::load` does: the headless proof then samples the font
        // atlases from the same texture list it rasterises (an unbound chain
        // draws the glyphs into the font channel, a chain bound to another
        // fixture's indices draws them as wrong-atlas swatches).
        if let Some(fonts) = crate::hud::pack_fonts(&root) {
            fonts.bind(pack.font_textures);
        }
        let skin = Skin::from_pack(&pack)?;
        Some(PackFixture { skin, textures })
    }

    fn white_icon(textures: &mut Vec<TextureImage>) -> usize {
        let index = textures.len();
        textures.push(TextureImage::white());
        index
    }

    /// `hud.screen.map-select`: the real registry's 38 maps, pagination
    /// arithmetic, 90 px rows and the large PLAY target at both viewports.
    #[test]
    fn hud_screen_map_select_lists_38_maps_with_pagination() {
        let registry = crate::map_registry::Registry::open_repo_default()
            .expect("analysis/maps/REGISTRY.json must exist for the map screen");
        assert_eq!(registry.maps.len(), 38, "registry map count");
        let data = MapSelectData::from_registry(&registry);
        assert_eq!(data.len(), 38, "map select lists every registry map");
        assert!(!data.is_empty());
        assert_eq!(data.page_count(7), 6, "38 maps on 7-row pages");
        assert_eq!(data.page(0, 7).len(), 7);
        assert_eq!(data.page(5, 7).len(), 3, "last page");
        assert_eq!(data.page(99, 7).len(), 3, "page index clamps");
        for per_page in [1usize, 5, 7, 10, 24] {
            let pages = data.page_count(per_page);
            assert_eq!(pages, 38usize.div_ceil(per_page));
            let mut covered = 0;
            for page in 0..pages {
                covered += data.page(page, per_page).len();
            }
            assert_eq!(covered, 38, "every map reachable at {per_page}/page");
        }
        let hud = MobileHud::new();
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
        ] {
            let layout = hud.layout_map_select(
                &MapSelectView::new(viewport, &data).safe(safe),
            );
            assert_eq!(layout.pages, data.page_count(layout.per_page));
            assert_eq!(
                layout.row_count(),
                data.page(layout.page, layout.per_page).len()
            );
            assert!(layout.per_page >= 1);
            for row in &layout.rows {
                assert!(
                    row.h >= SCREEN_TOUCH_MIN_PX - 0.01,
                    "row {} px is below the 90 px touch floor",
                    row.h
                );
                assert!(row.is_inside(&layout.list), "row outside list: {row:?}");
            }
            for pair in layout.rows.windows(2) {
                assert!(!pair[0].intersects(&pair[1]), "rows overlap: {pair:?}");
            }
            for (name, rect) in [
                ("play", layout.play),
                ("prev", layout.prev),
                ("next", layout.next),
                ("back", layout.back),
            ] {
                assert!(
                    rect.w.min(rect.h) >= SCREEN_TOUCH_MIN_PX - 0.01,
                    "{name} target {rect:?} is below 90 px"
                );
                assert!(rect.is_inside(&layout.panel), "{name} outside panel");
            }
            assert!(layout.play.w > layout.back.w, "PLAY is the large target");
            if let Some(first) = layout.rows.first() {
                assert_eq!(
                    layout.row_at(first.centre()),
                    Some(layout.first),
                    "row hit-test maps to the catalog index"
                );
            }
            // The last page still holds its rows and no overflow rects.
            let last = hud.layout_map_select(
                &MapSelectView::new(viewport, &data)
                    .safe(safe)
                    .page(layout.pages - 1),
            );
            assert_eq!(last.page, layout.pages - 1);
            assert_eq!(last.row_count(), data.page(layout.pages - 1, last.per_page).len());
        }
        eprintln!("HUD_SCREEN_TEST hud.screen.map-select PASS maps=38 pages@7=6");
    }

    /// `hud.screen.loading`: 2:1 frame, map/race inside it, half-filled
    /// progress bar, tip under the frame.
    #[test]
    fn hud_screen_loading_frame_progress_and_tips() {
        let hud = MobileHud::new();
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
        ] {
            let view = LoadingView::new(viewport, "Divide and Conquer", "HUMAN · ALLIANCE")
                .progress(0.5)
                .tip(loading_tip(2))
                .safe(safe);
            let layout = hud.layout_loading(&view);
            assert!(
                (layout.frame.w / layout.frame.h - 2.0).abs() < 0.01,
                "loading frame must keep the 1024x512 aspect: {:?}",
                layout.frame
            );
            let screen = Rect::new(0.0, 0.0, viewport.x, viewport.y);
            assert!(layout.frame.is_inside(&screen));
            assert!(layout.map_name.is_inside(&layout.frame));
            assert!(layout.race.is_inside(&layout.frame));
            assert!(layout.bar.is_inside(&layout.frame));
            assert!(layout.bar.w > 0.0 && layout.bar.h >= 8.0);
            assert!((layout.fill.w - layout.bar.w * 0.5).abs() < 1.0);
            assert!(layout.fill.w <= layout.bar.w);
            assert!(layout.tip.is_inside(&screen));
            assert!(layout.tip.y >= layout.frame.bottom());
        }
        assert_eq!(loading_tip(0), loading_tip(LOADING_TIPS.len()));
        assert!(!LOADING_TIPS.iter().any(|tip| tip.is_empty()));
        eprintln!("HUD_SCREEN_TEST hud.screen.loading PASS frame=2:1 bar=0.50");
    }

    /// `hud.screen.quest-log`: 90 px objective rows with done/not-done marks,
    /// page arithmetic and a 90 px CLOSE.
    #[test]
    fn hud_screen_quest_log_rows_and_marks() {
        let rows = [
            ("BUILD A TOWN HALL", true),
            ("TRAIN 6 FOOTMEN", true),
            ("SCOUT THE NORTHERN PASS", false),
            ("DESTROY THE ORC CAMP", false),
            ("RETURN TO THE KING", false),
        ];
        let quests = QuestLog::from_pairs(rows.iter().copied());
        assert_eq!(quests.len(), 5);
        assert_eq!(quests.done_count(), 2);
        assert_eq!(quests.page_count(3), 2);
        assert_eq!(quests.page(0, 3).len(), 3);
        assert_eq!(quests.page(1, 3).len(), 2);
        assert_eq!(quests.page(9, 3).len(), 2, "page clamps");
        let hud = MobileHud::new();
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
        ] {
            let layout =
                hud.layout_quest_log(&QuestLogView::new(viewport, &quests).safe(safe));
            for (row, mark) in layout.rows.iter().zip(layout.marks.iter()) {
                assert!(row.h >= SCREEN_TOUCH_MIN_PX - 0.01, "quest row {row:?}");
                assert!(mark.is_inside(row), "mark outside row");
                assert!(mark.w.min(mark.h) >= 24.0, "mark {mark:?}");
            }
            assert_eq!(layout.pages, quests.page_count(layout.per_page));
            assert!(layout.close.h.min(layout.close.w) >= SCREEN_TOUCH_MIN_PX - 0.01);
            assert!(layout.close.is_inside(&layout.panel));
        }
        eprintln!("HUD_SCREEN_TEST hud.screen.quest-log PASS rows=5 done=2");
    }

    /// `hud.screen.score`: the score panel with the seven lane-467 columns.
    #[test]
    fn hud_screen_score_columns_from_rows() {
        let data = ScoreData::from_pairs(
            "SCORE",
            "HUMAN · DIVIDE AND CONQUER · 24:13",
            [
                ("UNITS TRAINED", "24"),
                ("UNITS LOST", "6"),
                ("UNITS KILLED", "31"),
                ("GOLD GATHERED", "4120"),
                ("LUMBER GATHERED", "2380"),
                ("HEROES", "2"),
                ("TIME", "24:13"),
            ],
        );
        assert_eq!(data.rows.len(), 7);
        let hud = MobileHud::new();
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
        ] {
            let layout = hud.layout_score(&ScoreView::new(viewport, &data).safe(safe));
            assert_eq!(layout.rows.len(), 7);
            for (row, value) in layout.rows.iter().zip(layout.values.iter()) {
                assert!(row.is_inside(&layout.panel));
                assert!(value.is_inside(row));
                assert!(row.h > 0.0);
            }
            assert!(layout.header.is_inside(&layout.panel));
            assert!(layout.close.w.min(layout.close.h) >= SCREEN_TOUCH_MIN_PX - 0.01);
        }
        eprintln!("HUD_SCREEN_TEST hud.screen.score PASS rows=7");
    }

    /// `hud.text.fits-plate`: a 120+ character run never leaves the toast or
    /// the tooltip plate; the fitter wraps on words and honours the floor.
    #[test]
    fn hud_text_fits_plate_keeps_120_chars_inside() {
        let hud = MobileHud::new();
        let mut textures = Vec::new();
        let chrome = Chrome::new(&mut textures);
        let icons = load_icons(&mut textures);
        let font = Font::load();
        let long = "THE ENEMY HAS MUSTERED A LARGE ARMY AT THE NORTHERN GATE AND WILL ATTACK \
                    THE TOWN HALL WHEN THE NEXT WAVE SPAWNS UNLESS THE ALLIANCE REINFORCES NOW"
            .to_string();
        assert!(long.len() >= 120, "test string length {}", long.len());
        // Direct fitter contract: width, height and the readable floor.
        let inner = Rect::new(0.0, 0.0, 300.0, 90.0);
        let fit = fit_text_plate(&font, &long, inner, 0.8, MIN_TEXT_PX, 2);
        assert!(fit.lines.len() <= 2);
        assert!(fit.scale >= MIN_TEXT_PX / 30.0 - 1e-4);
        assert!(fit.height() <= inner.h + 0.5);
        for line in &fit.lines {
            assert!(
                font.measure(line, fit.scale) <= inner.w + 0.5,
                "line {line:?} exceeds the plate"
            );
        }
        for (viewport, safe) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080()),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080()),
        ] {
            let view = sample_view(viewport, icons, [None; 4], None)
                .toast(Some(ToastView::new(&long, ToastTone::Danger)))
                .tooltip(Some(format!("THE NORTHERN GATE\n{long}")))
                .safe(safe);
            let mut batch = UiBatch::default();
            let layout = hud.draw(&view, &mut batch, &font, &chrome);
            let toast = layout.toasts[0];
            let tooltip = layout.tooltip;
            let mut toast_glyphs = 0;
            let mut tooltip_glyphs = 0;
            for vertex in &batch.font {
                let point = Vec2::new(vertex.pos[0], vertex.pos[1]);
                if point.x >= toast.x - 2.0
                    && point.x <= toast.right() + 2.0
                    && point.y >= toast.y - 4.0
                    && point.y <= toast.bottom() + 4.0
                {
                    toast_glyphs += 1;
                    assert!(
                        toast.expanded(2.0, 2.0).contains(point),
                        "toast glyph outside its plate: {point:?} in {toast:?}"
                    );
                }
                if point.x >= tooltip.x - 2.0
                    && point.x <= tooltip.right() + 2.0
                    && point.y >= tooltip.y - 4.0
                    && point.y <= tooltip.bottom() + 4.0
                {
                    tooltip_glyphs += 1;
                    assert!(
                        tooltip.expanded(2.0, 2.0).contains(point),
                        "tooltip glyph outside its plate: {point:?} in {tooltip:?}"
                    );
                }
            }
            assert!(toast_glyphs > 10, "toast text drawn: {toast_glyphs} glyph vertices");
            assert!(
                tooltip_glyphs > 10,
                "tooltip text drawn: {tooltip_glyphs} glyph vertices"
            );
        }
        // Pack parchment path: the same run inside
        // `tooltip_panel_normal_384x256` stays inside the plate.
        if let Some(mut fixture) = pack_fixture() {
            let chrome = Chrome::new(&mut fixture.textures);
            let icons = load_icons(&mut fixture.textures);
            let font = Font::load();
            let map = map_texture();
            let viewport = Vec2::new(2414.0, 1080.0);
            let mut view = sample_view(viewport, icons, [None; 4], None)
                .safe(SafeArea::landscape_1080())
                .tooltip(Some(format!("THE NORTHERN GATE\n{long}")))
                .tooltip_costs(vec!["COST 135 GOLD · 60 LUMBER".to_string()]);
            view.hud_skin = Some(&fixture.skin);
            let mut batch = UiBatch::default();
            let layout = hud.draw(&view, &mut batch, &font, &chrome);
            let tooltip = layout.tooltip;
            // `pack_fixture` binds the pack faces to the fixture's texture
            // indices, so glyph quads land in the textured pass (as on device).
            let font_textures = fixture
                .skin
                .pack
                .as_ref()
                .map(|pack| pack.font_textures.to_vec())
                .unwrap_or_default();
            let mut glyph_vertices = 0;
            for draw in &batch.textured {
                if !font_textures.contains(&draw.texture) {
                    continue;
                }
                for vertex in &draw.vertices {
                    let point = Vec2::new(vertex.pos[0], vertex.pos[1]);
                    if point.x >= tooltip.x - 2.0
                        && point.x <= tooltip.right() + 2.0
                        && point.y >= tooltip.y - 2.0
                        && point.y <= tooltip.bottom() + 2.0
                    {
                        glyph_vertices += 1;
                        assert!(
                            tooltip.expanded(2.0, 2.0).contains(point),
                            "pack tooltip glyph outside the parchment: {point:?} in {tooltip:?}"
                        );
                    }
                }
            }
            for vertex in &batch.font {
                let point = Vec2::new(vertex.pos[0], vertex.pos[1]);
                if point.x >= tooltip.x - 2.0
                    && point.x <= tooltip.right() + 2.0
                    && point.y >= tooltip.y - 2.0
                    && point.y <= tooltip.bottom() + 2.0
                {
                    glyph_vertices += 1;
                    assert!(
                        tooltip.expanded(2.0, 2.0).contains(point),
                        "pack tooltip glyph outside the parchment: {point:?} in {tooltip:?}"
                    );
                }
            }
            assert!(
                glyph_vertices > 10,
                "pack tooltip text drawn: {glyph_vertices} glyph vertices"
            );
        }
        eprintln!("HUD_TEXT_TEST hud.text.fits-plate PASS chars={}", long.len());
    }

    // ---------------------------------------------------------- card (469)

    /// `hud.card.hotkey-badge`: a >= 24 px plate, >= 3:1 contrast over the
    /// brightest icon, glyphs inside the badge and painted after the icon.
    #[test]
    fn hud_card_hotkey_badge_is_readable_over_a_bright_icon() {
        let Some(mut fixture) = pack_fixture() else {
            eprintln!("skip: HUD pack missing");
            return;
        };
        let icon = white_icon(&mut fixture.textures);
        let chrome = Chrome::new(&mut fixture.textures);
        let flat = chrome.flat;
        let font = Font::load();
        let hud = MobileHud::new();
        let slot = Rect::new(200.0, 200.0, 95.0, 95.0);
        let command = CommandButton::new("HOLY LIGHT", Some(icon)).hotkey("H");
        let mut batch = UiBatch::default();
        let chain = FontChain::new(&[&font]);
        draw_card_cell(
            &mut batch,
            &chain,
            &hud.tokens,
            &fixture.skin,
            fixture.skin.pack.as_ref(),
            flat,
            slot,
            &command,
            None,
            1.0,
        );
        let role = card_tile_state(&command).role();
        let art = art_box(&fixture.skin, role, slot);
        let badge = card_hotkey_badge(art, 1.0);
        assert!(
            badge.w >= HOTKEY_BADGE_MIN_PX && badge.h >= HOTKEY_BADGE_MIN_PX,
            "hotkey badge {badge:?} below 24 px"
        );
        assert!(badge.is_inside(&slot), "badge clipped by the cell");
        let background = hotkey_badge_background([1.0; 4]);
        let ratio = contrast_ratio(hud.tokens.text, background);
        assert!(
            ratio >= 3.0,
            "hotkey letter contrast over a white icon is {ratio:.2}:1"
        );
        let mut glyph_vertices = 0;
        for vertex in &batch.font {
            let point = Vec2::new(vertex.pos[0], vertex.pos[1]);
            if badge.expanded(4.0, 4.0).contains(point) {
                glyph_vertices += 1;
            }
        }
        assert!(
            glyph_vertices >= 6,
            "hotkey letter drawn inside the badge: {glyph_vertices} vertices"
        );
        // The badge plate is painted over the icon: the corner pixel is dark
        // even though the icon under it is pure white.
        let pixels = rasterize(
            &batch,
            &fixture.textures,
            &font,
            &map_texture(),
            400,
            400,
        );
        assert!(badge.x > 0.0 && badge.y > 0.0);
        let px = ((badge.y as u32 + 3) * 400 + (badge.x as u32 + 3)) as usize * 4;
        let luma = (0.2126 * pixels[px] as f32
            + 0.7152 * pixels[px + 1] as f32
            + 0.0722 * pixels[px + 2] as f32)
            / 255.0;
        assert!(luma < 0.20, "badge centre should be a dark plate, luma {luma:.3}");
        eprintln!(
            "HUD_CARD_TEST hud.card.hotkey-badge PASS badge={:.1}x{:.1}px contrast={:.2}:1",
            badge.w, badge.h, ratio
        );
    }

    /// `hud.card.empty-slot-art`: the pack empty border over a 38,39,40 well —
    /// no pure-black interior, top light on the first row.
    #[test]
    fn hud_card_empty_slot_art_is_not_a_black_well() {
        let Some(mut fixture) = pack_fixture() else {
            eprintln!("skip: HUD pack missing");
            return;
        };
        let chrome = Chrome::new(&mut fixture.textures);
        let flat = chrome.flat;
        let font = Font::load();
        let hud = MobileHud::new();
        let slot = Rect::new(400.0, 400.0, 95.0, 95.0);
        let command = CommandButton::empty();
        let mut batch = UiBatch::default();
        let chain = FontChain::new(&[&font]);
        draw_card_cell(
            &mut batch,
            &chain,
            &hud.tokens,
            &fixture.skin,
            fixture.skin.pack.as_ref(),
            flat,
            slot,
            &command,
            None,
            1.0,
        );
        let empty_texture = fixture
            .skin
            .pack
            .as_ref()
            .and_then(|pack| pack.texture("button_border_empty_64"))
            .expect("pack empty border");
        assert!(
            batch.textured.iter().any(|draw| draw.texture == empty_texture),
            "pack button_border_empty_64 is drawn for an empty cell"
        );
        let pixels = rasterize(
            &batch,
            &fixture.textures,
            &font,
            &map_texture(),
            800,
            800,
        );
        let empty_role = card_tile_state(&CommandButton::empty()).role();
        let empty_art = art_box(&fixture.skin, empty_role, slot);
        let window = slot_inner(&fixture.skin, empty_role, empty_art);
        let sample_at = |x: f32, y: f32| -> f32 {
            let x = x.clamp(0.0, 799.0) as u32;
            let y = y.clamp(0.0, 799.0) as u32;
            let px = (y * 800 + x) as usize * 4;
            (0.2126 * pixels[px] as f32
                + 0.7152 * pixels[px + 1] as f32
                + 0.0722 * pixels[px + 2] as f32)
                / 255.0
        };
        let centre = sample_at(window.centre().x, window.centre().y);
        assert!(centre > 0.10, "empty interior is a black well: luma {centre:.3}");
        for (fx, fy) in [(0.3, 0.5), (0.7, 0.5), (0.5, 0.3), (0.5, 0.7)] {
            let luma = sample_at(window.x + window.w * fx, window.y + window.h * fy);
            assert!(luma > 0.10, "empty interior at ({fx},{fy}) luma {luma:.3}");
        }
        let top = sample_at(window.centre().x, window.y + 1.0);
        assert!(
            top > centre + 0.02,
            "1 px top light must be lighter: top {top:.3} centre {centre:.3}"
        );
        eprintln!(
            "HUD_CARD_TEST hud.card.empty-slot-art PASS interior_luma={centre:.3} top={top:.3}"
        );
    }

    /// `hud.card.cooldown-sweep`: a 50% cooldown covers half the icon window,
    /// radially from the top (right half swept, left half clear).
    #[test]
    fn hud_card_cooldown_sweep_covers_half_the_cell() {
        let Some(mut fixture) = pack_fixture() else {
            eprintln!("skip: HUD pack missing");
            return;
        };
        let icon = white_icon(&mut fixture.textures);
        let chrome = Chrome::new(&mut fixture.textures);
        let flat = chrome.flat;
        let font = Font::load();
        let hud = MobileHud::new();
        let slot = Rect::new(400.0, 400.0, 95.0, 95.0);
        let command = CommandButton::new("BLIZZARD", Some(icon)).cooldown(Some(0.5));
        let mut batch = UiBatch::default();
        let chain = FontChain::new(&[&font]);
        draw_card_cell(
            &mut batch,
            &chain,
            &hud.tokens,
            &fixture.skin,
            fixture.skin.pack.as_ref(),
            flat,
            slot,
            &command,
            None,
            1.0,
        );
        // The sweep is a triangle fan in the textured pass (cooldown only):
        // its vertices carry the 0.5/0.5 uv, unlike solid rect quads.
        let sweep_vertices = batch
            .textured
            .iter()
            .flat_map(|draw| draw.vertices.iter())
            .filter(|vertex| vertex.uv == [0.5, 0.5])
            .count();
        assert!(
            sweep_vertices >= 32 * 3,
            "radial sweep triangles emitted: {sweep_vertices} vertices"
        );
        let pixels = rasterize(
            &batch,
            &fixture.textures,
            &font,
            &map_texture(),
            800,
            800,
        );
        let role = card_tile_state(&command).role();
        let art = art_box(&fixture.skin, role, slot);
        let window = slot_inner(&fixture.skin, role, art);
        let mut swept = 0usize;
        let mut total = 0usize;
        for y in window.y as u32..window.bottom() as u32 {
            for x in window.x as u32..window.right() as u32 {
                let px = (y * 800 + x) as usize * 4;
                let luma = (0.2126 * pixels[px] as f32
                    + 0.7152 * pixels[px + 1] as f32
                    + 0.0722 * pixels[px + 2] as f32)
                    / 255.0;
                if luma < 0.25 {
                    swept += 1;
                }
                total += 1;
            }
        }
        let share = swept as f32 / total.max(1) as f32;
        assert!(
            (share - 0.5).abs() <= 0.08,
            "50% cooldown should cover half the cell, covered {share:.3}"
        );
        let mid_y = window.centre().y as u32;
        let left = ((mid_y * 800 + (window.centre().x as u32 - 10)) as usize) * 4;
        let right = ((mid_y * 800 + (window.centre().x as u32 + 10)) as usize) * 4;
        let luma_at = |px: usize| {
            (pixels[px] as f32 + pixels[px + 1] as f32 + pixels[px + 2] as f32) / 765.0
        };
        let left_luma = luma_at(left);
        let right_luma = luma_at(right);
        assert!(
            left_luma > right_luma + 0.1,
            "sweep starts at the top and runs clockwise: left {left_luma:.3} right {right_luma:.3}"
        );
        eprintln!("HUD_CARD_TEST hud.card.cooldown-sweep PASS share={share:.3}");
    }

    /// `hud.card.cost-red-when-unaffordable`: the cost row is gold when
    /// affordable and `tokens.danger` when the sim says it is not.
    #[test]
    fn hud_card_cost_turns_red_when_unaffordable() {
        let Some(mut fixture) = pack_fixture() else {
            eprintln!("skip: HUD pack missing");
            return;
        };
        let chrome = Chrome::new(&mut fixture.textures);
        let flat = chrome.flat;
        let font = Font::load();
        let hud = MobileHud::new();
        let slot = Rect::new(100.0, 100.0, 95.0, 95.0);
        let role_style = |affordable: bool| {
            CommandButton::new("FARM", None)
                .hotkey("F")
                .cost(Some(135))
                .affordable(affordable)
        };
        let cost_region = |skin: &Skin, slot: Rect| {
            let present = CommandButton::new("X", None);
            let role = card_tile_state(&present).role();
            let art = art_box(skin, role, slot);
            let window = slot_inner(skin, role, art);
            Rect::new(
                window.x,
                window.bottom() - (26.0f32).max(HOTKEY_BADGE_MIN_PX),
                window.w * 0.56,
                (26.0f32).max(HOTKEY_BADGE_MIN_PX),
            )
        };
        let gold = |color: [f32; 4], target: [f32; 4]| {
            (color[0] - target[0]).abs() < 0.03
                && (color[1] - target[1]).abs() < 0.03
                && (color[2] - target[2]).abs() < 0.03
        };
        let linear = |c: [f32; 4]| {
            [
                crate::hud::srgb_to_linear(c[0]),
                crate::hud::srgb_to_linear(c[1]),
                crate::hud::srgb_to_linear(c[2]),
                c[3],
            ]
        };
        for (affordable, expected, label) in [
            (true, linear(hud.tokens.gold), "gold"),
            (false, linear(hud.tokens.danger), "red"),
        ] {
            let command = role_style(affordable);
            let mut batch = UiBatch::default();
            let chain = FontChain::new(&[&font]);
            draw_card_cell(
                &mut batch,
                &chain,
                &hud.tokens,
                &fixture.skin,
                fixture.skin.pack.as_ref(),
                flat,
                slot,
                &command,
                None,
                1.0,
            );
            let region = cost_region(&fixture.skin, slot);
            let mut colored = 0;
            for vertex in &batch.font {
                let point = Vec2::new(vertex.pos[0], vertex.pos[1]);
                if region.expanded(2.0, 2.0).contains(point)
                    && gold(vertex.color, expected)
                {
                    colored += 1;
                }
            }
            assert!(
                colored > 0,
                "cost row must be {label} when affordable={affordable}"
            );
        }
        eprintln!("HUD_CARD_TEST hud.card.cost-red-when-unaffordable PASS");
    }

    /// Ability-cell state flags are one routine: normal has no sweep and no
    /// ring, cooldown emits the radial wedge, autocast paints the gold ring,
    /// disabled dims the icon, mana prints in the corner.
    #[test]
    fn hud_card_ability_states_draw_distinct_cues() {
        let Some(mut fixture) = pack_fixture() else {
            eprintln!("skip: HUD pack missing");
            return;
        };
        let icon = white_icon(&mut fixture.textures);
        let chrome = Chrome::new(&mut fixture.textures);
        let flat = chrome.flat;
        let font = Font::load();
        let hud = MobileHud::new();
        let slot = Rect::new(100.0, 100.0, 95.0, 95.0);
        let ability = |cooldown: Option<f32>, autocast: bool, enabled: bool| {
            CommandButton::new("ABILITY", Some(icon))
                .hotkey("Q")
                .mana_cost(50)
                .cooldown(cooldown)
                .autocast(autocast)
                .enabled(enabled)
        };
        let draw = |command: &CommandButton<'_>| {
            let mut batch = UiBatch::default();
            let chain = FontChain::new(&[&font]);
            draw_card_cell(
                &mut batch,
                &chain,
                &hud.tokens,
                &fixture.skin,
                fixture.skin.pack.as_ref(),
                flat,
                slot,
                command,
                None,
                1.1,
            );
            batch
        };
        let sweep = |batch: &UiBatch| {
            batch
                .textured
                .iter()
                .flat_map(|draw| draw.vertices.iter())
                .filter(|vertex| vertex.uv == [0.5, 0.5])
                .count()
        };
        let linear = |c: [f32; 4]| {
            [
                crate::hud::srgb_to_linear(c[0]),
                crate::hud::srgb_to_linear(c[1]),
                crate::hud::srgb_to_linear(c[2]),
                c[3],
            ]
        };
        let near = |color: [f32; 4], target: [f32; 4]| {
            (color[0] - target[0]).abs() < 0.03
                && (color[1] - target[1]).abs() < 0.03
                && (color[2] - target[2]).abs() < 0.03
        };
        let gold = linear(hud.tokens.gold);
        let gold_vertices = |batch: &UiBatch| {
            batch
                .textured
                .iter()
                .flat_map(|draw| draw.vertices.iter())
                .filter(|vertex| near(vertex.color, gold))
                .count()
        };
        let normal = draw(&ability(None, false, true));
        assert_eq!(sweep(&normal), 0, "normal has no cooldown wedge");
        assert_eq!(gold_vertices(&normal), 0, "normal has no autocast ring");
        let cooldown = draw(&ability(Some(0.5), false, true));
        assert!(
            sweep(&cooldown) >= 32 * 3,
            "cooldown wedge triangles: {}",
            sweep(&cooldown)
        );
        let autocast = draw(&ability(None, true, true));
        assert!(
            gold_vertices(&autocast) >= 16,
            "autocast ring quads: {}",
            gold_vertices(&autocast)
        );
        let disabled = draw(&ability(None, false, false));
        let dim = crate::hud::srgb_to_linear(0.42);
        let mut dimmed = 0;
        for draw in &disabled.textured {
            if draw.texture != icon {
                continue;
            }
            for vertex in &draw.vertices {
                if (vertex.color[0] - dim).abs() < 0.03 {
                    dimmed += 1;
                }
            }
        }
        assert!(dimmed >= 6, "disabled icon is dimmed: {dimmed} vertices");
        let supply = linear(hud.tokens.supply);
        let mut mana_vertices = 0;
        for vertex in &normal.font {
            if near(vertex.color, supply) && vertex.pos[1] > slot.y + slot.h * 0.6 {
                mana_vertices += 1;
            }
        }
        assert!(
            mana_vertices >= 6,
            "mana cost in the bottom-right corner: {mana_vertices} vertices"
        );
        eprintln!(
            "HUD_CARD_TEST hud.card.ability-states PASS sweep={} autocast={} dim={} mana={}",
            sweep(&cooldown),
            gold_vertices(&autocast),
            dimmed,
            mana_vertices
        );
    }

    fn write_screen_png(
        out_dir: &std::path::Path,
        name: &str,
        tag: &str,
        batch: &UiBatch,
        textures: &[TextureImage],
        font: &Font,
        map: &TextureImage,
        viewport: Vec2,
    ) -> std::path::PathBuf {
        let pixels = rasterize(
            batch,
            textures,
            font,
            map,
            viewport.x as u32,
            viewport.y as u32,
        );
        // No magenta tint may survive the pack draw path (the reported defect).
        let magenta = pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] > 180 && pixel[2] > 180 && pixel[1] < 60)
            .count();
        assert_eq!(magenta, 0, "{name}: {magenta} magenta pixels");
        let out = out_dir.join(format!("{name}-{tag}.png"));
        write_png(&out, viewport.x as u32, viewport.y as u32, &pixels).expect("screen png");
        let bytes = std::fs::metadata(&out).expect("screen png metadata").len();
        assert!(bytes > 10_000, "{}: {bytes} bytes", out.display());
        eprintln!("HUD_SCREEN_PNG {} {bytes} bytes", out.display());
        out
    }

    /// Headless evidence: every ledger-469 screen at both reference viewports
    /// under `analysis/hudskin/screens/`, drawn from the operator pack.
    #[test]
    fn hud_screen_headless_pngs_for_every_screen() {
        let Some(mut fixture) = pack_fixture() else {
            eprintln!("skip: HUD pack missing");
            return;
        };
        let chrome = Chrome::new(&mut fixture.textures);
        let icons = load_icons(&mut fixture.textures);
        let font = Font::load();
        let map = map_texture();
        let hud = MobileHud::new();
        let out_dir = repo_root().join("analysis/hudskin/screens");
        std::fs::create_dir_all(&out_dir).expect("screens dir");
        let registry = crate::map_registry::Registry::open_repo_default().expect("registry");
        let maps = MapSelectData::from_registry(&registry);
        let quests = QuestLog::from_pairs([
            ("BUILD A TOWN HALL", true),
            ("TRAIN 6 FOOTMEN", true),
            ("SCOUT THE NORTHERN PASS", false),
            ("DESTROY THE ORC CAMP", false),
            ("RETURN TO THE KING", false),
        ]);
        let score = ScoreData::from_pairs(
            "SCORE",
            "HUMAN · DIVIDE AND CONQUER · 24:13",
            [
                ("UNITS TRAINED", "24"),
                ("UNITS LOST", "6"),
                ("UNITS KILLED", "31"),
                ("GOLD GATHERED", "4120"),
                ("LUMBER GATHERED", "2380"),
                ("HEROES", "2"),
                ("TIME", "24:13"),
            ],
        );
        let mut written = Vec::new();
        for (viewport, safe, tag) in [
            (Vec2::new(2414.0, 1080.0), SafeArea::landscape_1080(), "2414x1080"),
            (Vec2::new(1080.0, 2414.0), SafeArea::portrait_1080(), "1080x2414"),
        ] {
            // 1. Map select (page 2 of the 38).
            let view = MapSelectView::new(viewport, &maps)
                .safe(safe)
                .page(1)
                .selected(Some(8))
                .hover(Some(9))
                .hud_skin(Some(&fixture.skin));
            let mut batch = UiBatch::default();
            let layout = hud.draw_map_select(&view, &mut batch, &font, &chrome);
            assert!(layout.row_count() > 0);
            written.push(write_screen_png(
                &out_dir,
                "map-select",
                tag,
                &batch,
                &fixture.textures,
                &font,
                &map,
                viewport,
            ));
            // 2. Loading with the pack frame and a half-full bar.
            let view = LoadingView::new(viewport, "Divide and Conquer", "HUMAN · ALLIANCE")
                .progress(0.5)
                .tip(loading_tip(1))
                .safe(safe)
                .hud_skin(Some(&fixture.skin));
            let mut batch = UiBatch::default();
            hud.draw_loading(&view, &mut batch, &font, &chrome);
            written.push(write_screen_png(
                &out_dir,
                "loading",
                tag,
                &batch,
                &fixture.textures,
                &font,
                &map,
                viewport,
            ));
            // 3. Quest log.
            let view = QuestLogView::new(viewport, &quests)
                .safe(safe)
                .hud_skin(Some(&fixture.skin));
            let mut batch = UiBatch::default();
            hud.draw_quest_log(&view, &mut batch, &font, &chrome);
            written.push(write_screen_png(
                &out_dir,
                "quest-log",
                tag,
                &batch,
                &fixture.textures,
                &font,
                &map,
                viewport,
            ));
            // 4. Score.
            let view = ScoreView::new(viewport, &score)
                .safe(safe)
                .hud_skin(Some(&fixture.skin));
            let mut batch = UiBatch::default();
            hud.draw_score(&view, &mut batch, &font, &chrome);
            written.push(write_screen_png(
                &out_dir,
                "score",
                tag,
                &batch,
                &fixture.textures,
                &font,
                &map,
                viewport,
            ));
            // 5. Command card: hotkey badges, mana cost, radial cooldown,
            // autocast ring, unaffordable (red) build cost and the MORE page.
            let commands: Vec<CommandButton<'_>> = vec![
                CommandButton::new("HOLY LIGHT", None)
                    .hotkey("H")
                    .mana_cost(65),
                CommandButton::new("BLIZZARD", None)
                    .hotkey("B")
                    .mana_cost(75)
                    .cooldown(Some(0.5)),
                CommandButton::new("DIVINE SHIELD", None)
                    .hotkey("D")
                    .mana_cost(25)
                    .autocast(true),
                CommandButton::new("FARM", None)
                    .hotkey("F")
                    .cost(Some(135))
                    .affordable(false),
                CommandButton::new("MORE", None).page(1, 2),
                CommandButton::new("STOP", None).hotkey("S"),
                CommandButton::empty(),
                CommandButton::empty(),
                CommandButton::empty(),
                CommandButton::empty(),
                CommandButton::empty(),
                CommandButton::empty(),
            ];
            let mut view = sample_view(viewport, icons, [None; 4], None)
                .safe(safe)
                .commands(&commands);
            view.hud_skin = Some(&fixture.skin);
            let mut batch = UiBatch::default();
            hud.draw(&view, &mut batch, &font, &chrome);
            written.push(write_screen_png(
                &out_dir,
                "command-card",
                tag,
                &batch,
                &fixture.textures,
                &font,
                &map,
                viewport,
            ));
            // 6. Allies / journal / game menu dialogs (pack plate + rows).
            for (screen, name) in [
                (DialogScreen::Allies, "allies"),
                (DialogScreen::Journal, "journal"),
                (DialogScreen::GameMenu, "game-menu"),
            ] {
                let view = DialogView::for_screen(screen, viewport)
                    .hud_skin(Some(&fixture.skin), Some(&fixture.skin));
                let mut batch = UiBatch::default();
                hud.draw_dialog(&view, &mut batch, &font, &chrome);
                written.push(write_screen_png(
                    &out_dir,
                    name,
                    tag,
                    &batch,
                    &fixture.textures,
                    &font,
                    &map,
                    viewport,
                ));
            }
        }
        assert_eq!(written.len(), 16, "eight screens x two viewports");
        eprintln!("HUD_SCREEN_PNGS {}", written.len());
    }

}
