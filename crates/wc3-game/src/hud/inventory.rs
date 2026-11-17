//! Inventory (backpack) panel: the WC3 2x3 item grid inside a framed plate.
//!
//! Mechanic id `ui.inventory`. The retail console puts a small framed panel
//! labelled **Inventory** between the unit card and the 4x3 command card: six
//! item slots, 2 columns x 3 rows, each slot an ornamented stone frame with a
//! square icon window (`ReplaceableTextures\CommandButtons\BTN*.blp`). The sim
//! already carries the model this panel displays — `Unit::items` is a six-entry
//! slot list of `ItemId`, `Sim::items` holds `Item { id, def_index, pos, owner,
//! charges }` and `Item::def()` resolves the shipped `ItemDef` — so this module
//! is purely the screen side: geometry, frames, finger targets and icon names.
//!
//! Contract with the caller (same as every other `hudskin` region):
//!
//! * the caller passes the panel rect it laid out; [`draw`] paints the plate,
//!   the header cartouche and the six slot frames, and returns [`InventoryParts`]
//!   with the frame, the header box, every slot rect, each slot's 1:1 icon
//!   window, its charge band and its finger target;
//! * nothing here draws text or icons. The header label ("INVENTORY"), the item
//!   icon and the charge count are the caller's (`Font` + texture index live
//!   there), exactly like `hudskin::army` chips;
//! * [`hit`] / [`classify`] / [`drag_target`] turn finger input into a slot
//!   action, and [`icon_for`] resolves an item's command-button icon name from
//!   the object data so the caller can bind a texture to it.
//!
//! # Touch contract (phone-first, no keyboard)
//!
//! This HUD runs on a 2414x1080 phone at ~2.625x density, so 48 dp is **126
//! device px**: every actionable target on the panel is at least that size.
//!
//! * the slot frames are laid out at [`TOUCH_MIN`] (126 px) — [`touch_panel_rect`]
//!   builds that panel — and the seams between them are folded into the slots'
//!   [`InventoryParts::touches`] rects (pitch cell = slot + seam, 131.5 px at
//!   scale 1.1), so a finger that lands on the 5.5 px seam still hits a slot;
//! * **tap** on a slot = activate it ([`DragAction::Use`]: use the carried item,
//!   or take the buy when a shop is selected), **drag** to another slot = move
//!   ([`DragAction::Move`]), **drag** off the panel (to the world, or onto a
//!   trash chip the caller draws) = drop ([`DragAction::Drop`]);
//! * nothing needs a right click, a hotkey or a keyboard: the module has no
//!   key paths at all. The caller arms a drag after [`DRAG_THRESHOLD`] px of
//!   travel or [`LONG_PRESS_SECS`] of hold, and calls [`classify`] with
//!   [`Gesture::Drag`] once armed (a finger that lifts before that is a
//!   [`Gesture::Tap`]).
//!
//! Geometry is derived from the panel rect, never from screen constants: the
//! six slots are square at any panel size (portrait phones included), they never
//! overlap, and they stay inside the frame. [`panel_rect`] is the exact inverse
//! — give it the slot side and it returns the panel rect whose grid is exactly
//! that side.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use glam::Vec2;

use crate::hud::{Rect, UiBatch};
use crate::hudskin::{dialogs, Skin};

/// Carried-item slots: WC3's hero inventory is 2 columns x 3 rows.
pub const SLOTS: usize = 6;

/// Grid columns (retail: two item columns beside the unit card).
pub const COLUMNS: usize = 2;

/// Grid rows.
pub const ROWS: usize = 3;

/// Minimum finger target, device px: 48 dp at the 2414x1080 device density
/// (~2.625x). Both the slot frame and its [`InventoryParts::touches`] rect are
/// at least this large on the touch panel ([`touch_panel_rect`]).
pub const TOUCH_MIN: f32 = 126.0;

/// Travel, device px, after which a touch on the panel becomes a drag. Below it
/// the gesture stays a tap (a fingertip wobbles 5-10 px on a 2.6x display).
pub const DRAG_THRESHOLD: f32 = 12.0;

/// Hold time, seconds, after which a touch arms a drag without travelling
/// (long-press then drag to the world = drop; no keyboard, no right click).
pub const LONG_PRESS_SECS: f32 = 0.45;

/// Framed plate the whole panel is drawn on (`dialog_panel`).
pub const ROLE_PANEL: &str = dialogs::ROLE_DIALOG;

/// Header cartouche that carries the "INVENTORY" caption (`wide_panel`).
pub const ROLE_HEADER: &str = dialogs::ROLE_BANNER;

/// Slot frame holding an item (`slot_gold`, 102x99 in the atlas).
pub const ROLE_SLOT: &str = "slot_gold";

/// Empty slot frame (`slot_dark`) — the same pair `hudskin::army` uses for
/// selected/unselected chips, so the two grids read as one HUD language.
pub const ROLE_SLOT_EMPTY: &str = "slot_dark";

/// Cooldown veil drawn over the icon window (`tile_dark`).
pub const ROLE_COOLDOWN: &str = "tile_dark";

/// Stone border between the panel edge and the content box, logical px (scaled).
pub const PANEL_PAD: f32 = 6.0;

/// Seam between two slots, logical px (scaled). Tighter than the command card's
/// 8 px pitch gap: the inventory slots read as one backpack block. The seam is
/// also the finger padding — it is inside the slot's touch rect, not a hole.
pub const SLOT_GAP: f32 = 5.0;

/// Header band height, **device px**: one HUD text line plus its bevel at a
/// legible size on both orientations (a scale-driven band became a 10 px strip
/// in portrait).
pub const HEADER_H: f32 = 22.0;

/// Gap between the header band and the first slot row, logical px (scaled).
pub const GRID_TOP_GAP: f32 = 2.0;

/// Share of the slot side taken by the frame on each side: the icon window is
/// `1 - 2 * ICON_INSET` of the slot, so it is always square, always inside the
/// slot and never smaller than 68% of it. The `slot_gold` art insets are 42/26
/// of a 102 px element (~41%), which would swallow a phone-sized slot; the
/// window is therefore the module's own geometry, capped like
/// `hudskin::army::chip_border` caps the portrait hole.
pub const ICON_INSET: f32 = 0.16;

/// Height share of the icon window reserved at its bottom edge for the charge
/// count the caller renders ([`InventoryParts::charges`]).
pub const CHARGE_BAND: f32 = 0.34;

/// Panel plate tint. The art already carries the gold bevel; the tint only sets
/// how far the plate sits back from the screen content.
pub const PANEL_TINT: [f32; 4] = dialogs::PANEL_TINT_DIALOG;

/// Header cartouche tint (matches the other banner strips).
pub const HEADER_TINT: [f32; 4] = dialogs::BANNER_TINT;

/// Cooldown veil multiplier over `tile_dark`: dark and slightly blue so the
/// item icon still reads through it (same value as `hudskin::tiles`).
pub const COOLDOWN_DARKEN: [f32; 4] = [0.25, 0.25, 0.30, 0.85];

/// Below this a scale is treated as degenerate instead of dividing by ~zero.
pub const MIN_SCALE: f32 = 0.01;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// One inventory slot's display state.
///
/// The sim side is `Unit::items[i]` -> `ItemId` -> `Sim::items` -> `Item`, so
/// the caller builds this per slot: `icon` is the command-button name resolved
/// by [`icon_for`] (or any name it prefers), `charges` is `Item::charges` and
/// `cooldown` is `0.0..=1.0` of the item's remaining cooldown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemSlot<'a> {
    /// Command-button icon name (`BTNAnkh`), drawn by the caller. `None` means
    /// the install has no icon for the item: the caller paints a grey
    /// placeholder (e.g. `skin.stretch(batch, "tile_empty", icon, grey)`).
    pub icon: Option<&'a str>,
    /// `Item::charges`; `0` = unlimited (no count is drawn), `> 0` = remaining
    /// uses ([`charge_label`]).
    pub charges: i32,
    /// Remaining cooldown as a `0.0..=1.0` share of the icon height; `0` = ready.
    pub cooldown: f32,
}

/// Everything [`draw`] resolves on screen: the pieces the caller still has to
/// fill (header text, item icons, charge counts) and the rects used for input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InventoryParts {
    /// Panel plate the module painted into (== the `rect` it was given).
    pub frame: Rect,
    /// Header cartouche box: the "INVENTORY" caption goes here.
    pub header: Rect,
    /// The six slot rects, row-major (row 0 = slots 0,1).
    pub slots: [Rect; SLOTS],
    /// Item window of each slot: square, strictly inside the slot frame.
    pub icons: [Rect; SLOTS],
    /// Bottom band of each icon window: right-align the charge count here.
    pub charges: [Rect; SLOTS],
    /// Finger target of each slot: the slot grown by half the seam on every
    /// side (the pitch cell), so the seams are not dead zones and adjacent
    /// targets share an edge instead of overlapping. At least [`TOUCH_MIN`] on
    /// the touch panel.
    pub touches: [Rect; SLOTS],
}

impl InventoryParts {
    /// Slot rect by index, `None` when the index is not a carried slot.
    pub fn slot(&self, index: usize) -> Option<Rect> {
        self.slots.get(index).copied()
    }

    /// True when `point` is inside the panel plate.
    pub fn contains(&self, point: Vec2) -> bool {
        self.frame.contains(point)
    }
}

/// A finger gesture that started on an inventory slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Lifted before the drag armed ([`DRAG_THRESHOLD`] / [`LONG_PRESS_SECS`]).
    Tap,
    /// Armed drag, released at the reported point.
    Drag,
}

/// What a gesture on the panel means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAction {
    /// Tap, or a drag released on the origin slot: **activate the slot**.
    ///
    /// The caller maps it by slot state: carried item -> `Sim::use_item` (or
    /// `Sim::use_item_on` when a target/point is armed), empty slot with a shop
    /// row selected -> `Sim::buy_item` (the sim appends the bought item to the
    /// first free slot, so the tapped slot is the confirmation target).
    Use(usize),
    /// Drag released on another slot: put/swap the carried item there.
    Move(usize),
    /// Released outside the panel (world or a trash chip): `Sim::drop_item`.
    Drop,
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// HUD art scale with the degenerate value pinned (`0`/NaN would collapse the
/// padding while the slot pitch stayed finite).
fn art_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.max(MIN_SCALE)
    } else {
        1.0
    }
}

/// A rect with non-finite fields replaced by zeros (a NaN panel must not poison
/// the batch geometry).
fn sanitise(rect: Rect) -> Rect {
    let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
    Rect::new(
        finite(rect.x),
        finite(rect.y),
        finite(rect.w).max(0.0),
        finite(rect.h).max(0.0),
    )
}

/// Panel rect whose 2x3 grid has `cell`-sided square slots.
///
/// The grid solves `pitch = (grid + seam) / count`, so
/// `w = 2 * cell + seam + 2 * pad` and
/// `h = 3 * cell + 2 * seam + top_gap + header + 2 * pad` are exact: feeding
/// the result back into [`plan`] yields slots of exactly `cell`.
///
/// Use [`touch_panel_rect`] for the finger-sized panel; this form is for a
/// deliberately smaller (retail-proportioned) plate.
pub fn panel_rect(x: f32, y: f32, cell: f32, scale: f32) -> Rect {
    let s = art_scale(scale);
    let pad = PANEL_PAD * s;
    let gap = SLOT_GAP * s;
    let cell = if cell.is_finite() { cell.max(0.0) } else { 0.0 };
    let w = COLUMNS as f32 * cell + gap + 2.0 * pad;
    let h = ROWS as f32 * cell
        + (ROWS as f32 - 1.0) * gap
        + GRID_TOP_GAP * s
        + HEADER_H
        + 2.0 * pad;
    Rect::new(x, y, w, h)
}

/// The finger-sized panel: every slot is exactly [`TOUCH_MIN`] device px, so
/// each slot and its touch rect clear the 48 dp minimum by construction.
///
/// At 2414x1080 (scale 1.1) this is 270.7 x 426.4 px: it fits the 436 px console
/// band with 9.6 px to spare (see `analysis/hudskin/inventory.md` for the full
/// arithmetic against the live console / card / portrait rects).
pub fn touch_panel_rect(x: f32, y: f32, scale: f32) -> Rect {
    panel_rect(x, y, TOUCH_MIN, scale)
}

/// Geometry of the panel inside `rect`, without painting anything.
///
/// [`draw`] is this plus the quads, and both use the same numbers, so a caller
/// can hit-test a panel it has not drawn yet (layout pass) and get identical
/// rects.
pub fn plan(rect: Rect, scale: f32) -> InventoryParts {
    let s = art_scale(scale);
    let frame = sanitise(rect);
    let pad = PANEL_PAD * s;
    let gap = SLOT_GAP * s;
    let content = frame.inset(pad, pad);
    let header_h = HEADER_H.min(content.h);
    let header = Rect::new(content.x, content.y, content.w, header_h);
    let below_header = (content.bottom() - header.bottom()).max(0.0);
    let grid_top = header.bottom() + (GRID_TOP_GAP * s).min(below_header);
    let grid = Rect::new(
        content.x,
        grid_top,
        content.w,
        (content.bottom() - grid_top).max(0.0),
    );
    // `pitch = (grid + seam) / count` is the exact inverse of `count * pitch - seam`,
    // so the grid fills the content box on whichever axis binds first.
    let pitch_x = (grid.w + gap) / COLUMNS as f32;
    let pitch_y = (grid.h + gap) / ROWS as f32;
    let pitch = pitch_x.min(pitch_y).max(0.0);
    let side = (pitch - gap).max(0.0);
    let grid_w = (COLUMNS as f32 * pitch - gap).max(0.0);
    let grid_h = (ROWS as f32 * pitch - gap).max(0.0);
    let x0 = grid.x + (grid.w - grid_w) * 0.5;
    let y0 = grid.y + (grid.h - grid_h) * 0.5;
    // The seam belongs to both neighbours' finger targets, half each: the pitch
    // cells tile the grid exactly (shared edges, no overlap, no dead seam).
    let half_seam = (pitch - side).max(0.0) * 0.5;

    let border = side * ICON_INSET;
    let band = side * (1.0 - 2.0 * ICON_INSET) * CHARGE_BAND;
    let mut slots = [Rect::new(x0, y0, side, side); SLOTS];
    let mut icons = [Rect::new(x0, y0, 0.0, 0.0); SLOTS];
    let mut charges = icons;
    let mut touches = [Rect::new(x0, y0, side, side); SLOTS];
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let index = row * COLUMNS + column;
            let slot = Rect::new(
                x0 + column as f32 * pitch,
                y0 + row as f32 * pitch,
                side,
                side,
            );
            let icon = slot.inset(border, border);
            let touch = clamp_rect(
                Rect::new(
                    slot.x - half_seam,
                    slot.y - half_seam,
                    side + 2.0 * half_seam,
                    side + 2.0 * half_seam,
                ),
                frame,
            );
            slots[index] = slot;
            icons[index] = icon;
            charges[index] = Rect::new(icon.x, icon.bottom() - band, icon.w, band);
            touches[index] = touch;
        }
    }
    InventoryParts {
        frame,
        header,
        slots,
        icons,
        charges,
        touches,
    }
}

/// `rect` clipped to `bounds` (a finger target never leaves the plate).
fn clamp_rect(rect: Rect, bounds: Rect) -> Rect {
    let x = rect.x.max(bounds.x);
    let y = rect.y.max(bounds.y);
    let right = rect.right().min(bounds.right()).max(x);
    let bottom = rect.bottom().min(bounds.bottom()).max(y);
    Rect::new(x, y, right - x, bottom - y)
}

/// Multiply two RGBA tints channel by channel.
fn mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

/// Remaining cooldown clamped to `0.0..=1.0` (NaN reads as ready).
fn remaining(cooldown: f32) -> f32 {
    if cooldown.is_finite() {
        cooldown.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Smaller side of the smallest finger target on the panel.
///
/// The device pass asserts `touch_size(&parts) >= TOUCH_MIN`; a panel drawn at
/// the retail slot size (80 px at 2414x1080) fails that on purpose, which is how
/// a composition regression is caught instead of shipping a 30 dp backpack.
pub fn touch_size(parts: &InventoryParts) -> f32 {
    parts.touches.iter().fold(f32::INFINITY, |min, rect| {
        min.min(rect.w.min(rect.h))
    })
}

/// True when every slot is a legal finger target ([`TOUCH_MIN`] on the smaller
/// side, 0.5 px of float slack).
pub fn touch_meets_minimum(parts: &InventoryParts) -> bool {
    let smallest = touch_size(parts);
    smallest.is_finite() && smallest >= TOUCH_MIN - 0.5
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// Draw the inventory panel into `rect` and return the rects the caller fills.
///
/// `items` is read per slot: a missing index or `None` paints an empty frame
/// (`slot_dark`), `Some` paints an occupied frame (`slot_gold`) plus the
/// cooldown veil. [`ItemSlot::icon`] is not drawn here — icons are textures the
/// caller owns — so occupying a slot never changes the quad count, only the
/// frame role.
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    items: &[Option<ItemSlot>],
    scale: f32,
    tint: [f32; 4],
) -> InventoryParts {
    let parts = plan(rect, scale);
    let s = art_scale(scale);
    if parts.frame.w <= 0.0 || parts.frame.h <= 0.0 {
        return parts;
    }
    dialogs::nine_scaled(skin, batch, ROLE_PANEL, parts.frame, s, mul(tint, PANEL_TINT));
    if parts.header.w > 0.0 && parts.header.h > 0.0 {
        dialogs::nine_scaled(skin, batch, ROLE_HEADER, parts.header, s, mul(tint, HEADER_TINT));
    }
    for index in 0..SLOTS {
        let slot = parts.slots[index];
        if slot.w <= 0.0 || slot.h <= 0.0 {
            continue;
        }
        let item = items.get(index).copied().flatten();
        let role = if item.is_some() {
            ROLE_SLOT
        } else {
            ROLE_SLOT_EMPTY
        };
        dialogs::nine_scaled(skin, batch, role, slot, s, tint);
        let Some(item) = item else {
            continue;
        };
        let left = remaining(item.cooldown);
        if left > 0.0 {
            let icon = parts.icons[index];
            let veil = Rect::new(icon.x, icon.y, icon.w, icon.h * left);
            if veil.w > 0.0 && veil.h > 0.0 {
                skin.stretch(batch, ROLE_COOLDOWN, veil, mul(tint, COOLDOWN_DARKEN));
            }
        }
    }
    parts
}

/// Charge count the caller renders in [`InventoryParts::charges`].
///
/// `charges > 0` is the remaining count; `0` means the item has no charges
/// (`uses 0` in `ItemData.slk` is "unlimited"), so no count is drawn.
pub fn charge_label(charges: i32) -> Option<String> {
    (charges > 0).then(|| charges.to_string())
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Slot under `point`, if any.
///
/// Tests the finger targets, not the frames: a tap on the seam between two
/// slots lands on the first of them instead of falling through. Targets share
/// edges and never overlap, so the first hit is the only hit.
pub fn hit(parts: &InventoryParts, point: Vec2) -> Option<usize> {
    parts
        .touches
        .iter()
        .position(|touch| touch.w > 0.0 && touch.h > 0.0 && touch.contains(point))
}

/// What releasing a drag that started in slot `from` at `point` means.
///
/// * release on another slot -> [`DragAction::Move`] (put/swap the item there);
/// * release on the origin slot -> [`DragAction::Use`] (a tap that drifted);
/// * release inside the panel but between slots / on the header -> `Move(from)`,
///   i.e. the item returns to its own slot (WC3 keeps the item when the drop
///   misses a slot but stays in the backpack);
/// * release outside the panel -> [`DragAction::Drop`] (item to the ground).
///
/// An out-of-range `from` is a caller bug; it resolves to `Drop` rather than
/// panicking, and the caller's drop path re-validates against the sim.
pub fn drag_target(parts: &InventoryParts, from: usize, point: Vec2) -> DragAction {
    drag_target_in(parts, from, point, None)
}

/// [`drag_target`] with a visible trash chip: a release inside `drop_zone` is a
/// [`DragAction::Drop`] even when the chip is drawn over the plate, because the
/// chip is on top and the finger is aiming at it. Pass `None` for the plain
/// drag-to-the-world behaviour.
pub fn drag_target_in(
    parts: &InventoryParts,
    from: usize,
    point: Vec2,
    drop_zone: Option<Rect>,
) -> DragAction {
    if from >= SLOTS {
        return DragAction::Drop;
    }
    if drop_zone.is_some_and(|zone| zone.w > 0.0 && zone.h > 0.0 && zone.contains(point)) {
        return DragAction::Drop;
    }
    match hit(parts, point) {
        Some(target) if target != from => DragAction::Move(target),
        Some(_) => DragAction::Use(from),
        None if parts.frame.contains(point) => DragAction::Move(from),
        None => DragAction::Drop,
    }
}

/// Classify one completed finger gesture that started on slot `from`.
///
/// A tap ignores the release point (a fingertip wobbles; a tap on slot 3 is
/// `Use(3)` even if it lifts 8 px away, which is under [`DRAG_THRESHOLD`]) and
/// activates the slot. An armed drag is classified by where it is released. The
/// caller arms the drag after [`DRAG_THRESHOLD`] px of travel or
/// [`LONG_PRESS_SECS`] of hold; there is no second button and no key path.
pub fn classify(
    parts: &InventoryParts,
    from: usize,
    point: Vec2,
    gesture: Gesture,
    drop_zone: Option<Rect>,
) -> DragAction {
    match gesture {
        Gesture::Tap if from < SLOTS => DragAction::Use(from),
        Gesture::Tap => DragAction::Drop,
        Gesture::Drag => drag_target_in(parts, from, point, drop_zone),
    }
}

// ---------------------------------------------------------------------------
// Icon resolution
// ---------------------------------------------------------------------------

/// Installed `Item::def_index` -> command-button icon name.
#[derive(Default)]
struct IconTable {
    by_def: HashMap<u16, &'static str>,
    /// Interned names, so a re-install never leaks a second copy.
    pool: Vec<&'static str>,
}

fn icon_table() -> &'static Mutex<IconTable> {
    static TABLE: OnceLock<Mutex<IconTable>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(IconTable::default()))
}

/// Icon basename of a WC3 art path (`…\CommandButtons\BTNAnkh.blp` -> `BTNAnkh`).
///
/// The object data stores art as a full BLP path; every HUD texture map
/// (`UiTheme::texture`) is keyed by the basename.
pub fn icon_basename(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let basename = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = basename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(basename);
    (!stem.is_empty()).then(|| stem.to_string())
}

/// Publish item icons, keyed by `Item::def_index`.
///
/// Returns how many rows the call stored. Names are interned (one copy per
/// distinct icon) and matched case-insensitively against what is already there.
pub fn install_icons<I, S>(entries: I) -> usize
where
    I: IntoIterator<Item = (u16, S)>,
    S: AsRef<str>,
{
    let Ok(mut table) = icon_table().lock() else {
        return 0;
    };
    let mut stored = 0;
    for (def_index, name) in entries {
        let name = name.as_ref().trim();
        let Some(name) = icon_basename(name) else {
            continue;
        };
        let interned = match table
            .pool
            .iter()
            .find(|pooled| pooled.eq_ignore_ascii_case(&name))
        {
            Some(existing) => *existing,
            None => {
                let leaked: &'static str = Box::leak(name.into_boxed_str());
                table.pool.push(leaked);
                leaked
            }
        };
        table.by_def.insert(def_index, interned);
        stored += 1;
    }
    stored
}

/// True when an icon table has been published ([`icon_for`] can answer).
pub fn icons_installed() -> bool {
    icon_table()
        .lock()
        .map(|table| !table.by_def.is_empty())
        .unwrap_or(false)
}

/// Publish the install's item icons from the object data (`Assets::objects`):
/// one row per `ItemData` entry that carries an `Art` icon.
///
/// Items without an icon in the install are left out, so [`icon_for`] answers
/// `None` for them and the caller draws its grey placeholder. Returns the number
/// of rows stored.
pub fn install_from_objects(objects: &wc3_data::GameDatabase) -> usize {
    let mut entries: Vec<(u16, String)> = Vec::new();
    for (index, def) in wc3_sim::data::ITEMS.iter().enumerate() {
        let Some(info) = objects.object(def.id) else {
            continue;
        };
        let Some(path) = info.icon.as_deref() else {
            continue;
        };
        let Some(name) = icon_basename(path) else {
            continue;
        };
        entries.push((index as u16, name));
    }
    install_icons(entries)
}

/// [`install_from_objects`], but only the first time: the HUD calls this once
/// per frame until the assets are in place, and it is an empty lock after that.
pub fn ensure_icons(objects: &wc3_data::GameDatabase) -> usize {
    if icons_installed() {
        0
    } else {
        install_from_objects(objects)
    }
}

/// Command-button icon name of an item, resolved through the object data by the
/// item's `def_index`.
///
/// `None` when the install has no icon for the item (the caller draws a grey
/// placeholder) or when no table has been published yet
/// ([`install_from_objects`] / [`ensure_icons`]).
pub fn icon_for(item: &wc3_sim::Item) -> Option<&'static str> {
    icon_table()
        .lock()
        .ok()?
        .by_def
        .get(&item.def_index)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::UiBatch;
    use crate::hudskin::tests_support::sample_skin;

    /// Live `MobileHud::layout` values, measured from `MobileHud::new().layout`
    /// on 2026-11-16 (2414x1080 landscape and 1080x2414 portrait, no skin).
    /// They are inputs to the fit arithmetic, so the report and the test agree.
    fn console_l() -> Rect {
        Rect::new(48.0, 596.004, 2318.0, 435.996)
    }
    fn card_l() -> Rect {
        Rect::new(1995.63, 686.017, 343.97, 255.97)
    }
    fn selection_l() -> Rect {
        Rect::new(491.18, 624.604, 493.35, 284.186)
    }
    fn groups_l() -> Rect {
        Rect::new(1240.38, 808.502, 499.4, 44.0)
    }
    fn console_p() -> Rect {
        Rect::new(24.0, 2085.068, 1032.0, 264.93)
    }
    const SCALE_L: f32 = 1.1;
    const SCALE_P: f32 = 0.5298;

    /// Live `mobile_hud` group geometry: ten plates, 6 * scale seam.
    const GROUP_PLATE: f32 = 44.0;
    const GROUP_GAP: f32 = 6.0;

    fn near(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn item_slot<'a>(icon: Option<&'a str>, charges: i32) -> ItemSlot<'a> {
        ItemSlot {
            icon,
            charges,
            cooldown: 0.0,
        }
    }

    fn occupied<'a>() -> [Option<ItemSlot<'a>>; SLOTS] {
        std::array::from_fn(|index| Some(item_slot(Some("BTNAnkh"), (index + 1) as i32)))
    }

    fn vertices(batch: &UiBatch) -> usize {
        batch.textured.iter().map(|draw| draw.vertices.len()).sum()
    }

    #[test]
    fn six_slots_in_a_two_by_three_grid() {
        for (scale, cell) in [(SCALE_L, TOUCH_MIN), (SCALE_L, 80.0), (SCALE_P, TOUCH_MIN)] {
            let parts = plan(panel_rect(100.0, 200.0, cell, scale), scale);
            assert_eq!(parts.slots.len(), SLOTS, "six carried slots");
            let pitch = parts.slots[1].x - parts.slots[0].x;
            assert!(pitch > 0.0, "column pitch {pitch}");
            for row in 0..ROWS {
                let first = parts.slots[row * COLUMNS];
                let second = parts.slots[row * COLUMNS + 1];
                assert!(near(second.x - first.x, pitch, 1e-3), "row {row} column pitch");
                assert!(near(second.y, first.y, 1e-4), "row {row} shares one y");
                if row > 0 {
                    let above = parts.slots[(row - 1) * COLUMNS];
                    assert!(
                        near(first.y - above.y, pitch, 1e-3),
                        "row {row} row pitch {} != column pitch {pitch}",
                        first.y - above.y
                    );
                }
            }
            for slot in parts.slots {
                assert!(near(slot.w, slot.h, 1e-4), "slots are square: {slot:?}");
                assert!(near(slot.w, cell, 1e-3), "slot side {} != cell {cell}", slot.w);
            }
        }
    }

    #[test]
    fn slots_never_overlap_and_stay_inside_the_frame() {
        for (scale, cell) in [(SCALE_L, TOUCH_MIN), (SCALE_P, TOUCH_MIN), (SCALE_L, 40.0)] {
            let rect = panel_rect(140.0, 260.0, cell, scale);
            let parts = plan(rect, scale);
            assert_eq!(parts.frame, rect, "frame is the rect the caller passed");
            // Pairwise disjoint, frames and finger targets alike.
            for (name, rects) in [("slot", parts.slots), ("touch", parts.touches)] {
                for a in 0..SLOTS {
                    for b in (a + 1)..SLOTS {
                        let (left, right) = (rects[a], rects[b]);
                        let disjoint = left.right() <= right.x + 1e-4
                            || right.right() <= left.x + 1e-4
                            || left.bottom() <= right.y + 1e-4
                            || right.bottom() <= left.y + 1e-4;
                        assert!(disjoint, "{name} {a} {left:?} and {b} {right:?} overlap");
                    }
                }
            }
            // Inside the plate, and below the header.
            for slot in parts.slots {
                assert!(
                    slot.x >= rect.x && slot.right() <= rect.right(),
                    "slot x {slot:?} outside {rect:?}"
                );
                assert!(
                    slot.y >= rect.y && slot.bottom() <= rect.bottom(),
                    "slot y {slot:?} outside {rect:?}"
                );
                assert!(slot.y >= parts.header.bottom(), "slot above the header");
            }
            for touch in parts.touches {
                assert!(
                    touch.x >= rect.x - 1e-4 && touch.right() <= rect.right() + 1e-4,
                    "touch x {touch:?} outside {rect:?}"
                );
                assert!(
                    touch.y >= rect.y - 1e-4 && touch.bottom() <= rect.bottom() + 1e-4,
                    "touch y {touch:?} outside {rect:?}"
                );
            }
            assert!(parts.header.y >= rect.y && parts.header.bottom() <= rect.bottom());
            assert!(parts.header.h > 0.0, "header band exists");
            // The icon window is strictly inside its slot and 1:1.
            for index in 0..SLOTS {
                let slot = parts.slots[index];
                let icon = parts.icons[index];
                assert!(icon.x > slot.x && icon.y > slot.y, "icon origin {icon:?}");
                assert!(
                    icon.right() < slot.right() && icon.bottom() < slot.bottom(),
                    "icon edge {icon:?}"
                );
                assert!(near(icon.w, icon.h, 1e-4), "icon window is 1:1: {icon:?}");
                let band = parts.charges[index];
                assert!(band.x >= icon.x && band.bottom() <= icon.bottom() + 1e-4);
                assert!(near(band.w, icon.w, 1e-4), "charge band spans the icon");
                assert!(band.h > 0.0 && band.h <= icon.h * CHARGE_BAND + 1e-4);
            }
        }
    }

    #[test]
    fn every_target_clears_48dp_at_2414x1080() {
        let parts = plan(touch_panel_rect(100.0, 300.0, SCALE_L), SCALE_L);
        // Slots themselves are 126 px and their finger targets are the pitch
        // cells: slot + one seam = 131.5 px.
        for (index, slot) in parts.slots.iter().enumerate() {
            assert!(
                slot.w.min(slot.h) >= TOUCH_MIN - 0.5,
                "slot {index} {:?} under 48 dp",
                (slot.w, slot.h)
            );
        }
        for (index, touch) in parts.touches.iter().enumerate() {
            assert!(
                touch.w.min(touch.h) >= TOUCH_MIN - 0.5,
                "touch {index} {:?} under 48 dp",
                (touch.w, touch.h)
            );
        }
        assert!(near(parts.slots[0].w, 126.0, 1e-3), "slot {} px", parts.slots[0].w);
        assert!(near(parts.touches[0].w, 131.5, 1e-3), "hit {}", parts.touches[0].w);
        // Padding per side = seam / 2 = 2.75 px at scale 1.1.
        let pad_x = (parts.touches[0].w - parts.slots[0].w) * 0.5;
        assert!(near(pad_x, 2.75, 1e-3), "touch padding {pad_x} px");
        assert!(touch_meets_minimum(&parts));
        assert!(near(touch_size(&parts), 131.5, 1e-3), "{}", touch_size(&parts));

        // The retail-proportioned panel (80 px slots) is below the minimum on
        // purpose: the gate has teeth instead of blessing 30 dp targets.
        let retail = plan(panel_rect(100.0, 300.0, 80.0, SCALE_L), SCALE_L);
        assert!(!touch_meets_minimum(&retail));
        assert!(touch_size(&retail) < TOUCH_MIN);
    }

    #[test]
    fn touch_panel_fits_the_console_and_clears_card_and_portrait() {
        let (console, card, selection) = (console_l(), card_l(), selection_l());
        // Landscape 2414x1080: the panel is right-aligned to the command card
        // and vertically centred in the console band.
        let panel = touch_panel_rect(0.0, 0.0, SCALE_L);
        let gap = 8.0 * SCALE_L;
        let x = card.x - gap - panel.w;
        let y = console.y + (console.h - panel.h) * 0.5;
        let panel = Rect::new(x, y, panel.w, panel.h);
        println!("landscape panel {panel:?} slot {:.1}", panel.w);

        // 426.4 px of a 436 px band: the three 126 px rows plus the frame fit.
        assert!(
            panel.h <= console.h,
            "panel {} taller than the console {}",
            panel.h,
            console.h
        );
        assert!(panel.h >= 3.0 * TOUCH_MIN, "three 126 px rows + chrome");
        assert!(panel.y >= console.y && panel.bottom() <= console.bottom());
        // Nothing else moves: no overlap with the command card ...
        assert!(panel.right() <= card.x, "panel {} over the card {}", panel.right(), card.x);
        assert!(near(card.x - panel.right(), gap, 1e-3), "8 * scale gap to the card");
        // ... nor with the unit card / portrait column.
        assert!(
            panel.x >= selection.right(),
            "panel {} over the selection column {}",
            panel.x,
            selection.right()
        );
        // ... and the group row still fits in the lane it keeps on the left:
        // ten 44 px plates and nine 6 * scale seams.
        let lane_left = selection.x + selection.w + gap;
        let lane_right = panel.x - gap;
        let needed = 10.0 * GROUP_PLATE + 9.0 * GROUP_GAP * SCALE_L;
        assert!(
            lane_right - lane_left >= needed,
            "group row needs {needed} of {}",
            lane_right - lane_left
        );
        // The current `groups_area` right edge (`card.x - gap`) overlaps the
        // panel by the measured margin, so the composition must re-bound it to
        // `panel.x - gap`. That single change is the wiring note for the
        // console rebuild; the lane above proves the row still fits there.
        let groups = groups_l();
        assert!(
            groups.right() > panel.x,
            "expected the live group row to overlap this panel: {} vs {}",
            groups.right(),
            panel.x
        );
        assert!(
            groups.w <= lane_right - lane_left,
            "group row {} wider than the re-bounded lane {}",
            groups.w,
            lane_right - lane_left
        );

        // Portrait 1080x2414: a 264.9 px band cannot host 378 px of slots, so the
        // panel floats above the console instead of shrinking under 48 dp.
        let portrait_console = console_p();
        assert!(
            portrait_console.h < ROWS as f32 * TOUCH_MIN,
            "portrait band {} cannot host three 126 px rows",
            portrait_console.h
        );
        let panel = touch_panel_rect(0.0, 0.0, SCALE_P);
        let floating = Rect::new(
            1080.0 - 24.0 - panel.w,
            portrait_console.y - gap - panel.h,
            panel.w,
            panel.h,
        );
        println!("portrait panel {floating:?} slot {:.1}", floating.w);
        assert!(floating.bottom() <= portrait_console.y, "sits above the console");
        assert!(floating.y >= 0.0 && floating.right() <= 1080.0 - 24.0 + 1e-3);
        for slot in plan(floating, SCALE_P).slots {
            assert!(near(slot.w, TOUCH_MIN, 1e-3), "portrait slot {:.1}", slot.w);
            assert!(near(slot.w, slot.h, 1e-4));
        }
    }

    #[test]
    fn hit_test_round_trip_for_every_slot() {
        for (scale, cell) in [(SCALE_L, TOUCH_MIN), (SCALE_P, TOUCH_MIN), (SCALE_L, 60.0)] {
            let parts = plan(panel_rect(80.0, 90.0, cell, scale), scale);
            assert_eq!(hit(&parts, Vec2::new(0.0, 0.0)), None, "outside the panel");
            for index in 0..SLOTS {
                let centre = parts.slots[index].centre();
                assert_eq!(hit(&parts, centre), Some(index), "slot {index} centre {centre:?}");
                assert_eq!(hit(&parts, parts.touches[index].centre()), Some(index));
                // Corners inside the half-open rect resolve to the same slot.
                let slot = parts.slots[index];
                assert_eq!(
                    hit(&parts, Vec2::new(slot.x + 0.5, slot.y + 0.5)),
                    Some(index)
                );
                assert_eq!(
                    hit(&parts, Vec2::new(slot.right() - 0.5, slot.bottom() - 0.5)),
                    Some(index)
                );
            }
            // The seam between two slots belongs to one of them, never to none.
            let seam = Vec2::new(
                (parts.slots[0].right() + parts.slots[1].x) * 0.5,
                parts.slots[0].centre().y,
            );
            assert!(matches!(hit(&parts, seam), Some(0) | Some(1)), "seam {seam:?}");
            // The header is inside the panel but on no slot.
            assert_eq!(hit(&parts, parts.header.centre()), None, "header is not a slot");
        }
    }

    #[test]
    fn tap_drag_and_long_press_classify_correctly() {
        let parts = plan(touch_panel_rect(100.0, 100.0, SCALE_L), SCALE_L);
        let zero = parts.slots[0].centre();
        let five = parts.slots[5].centre();
        // Tap: activates the slot it started on, whatever the release point.
        assert_eq!(classify(&parts, 0, zero, Gesture::Tap, None), DragAction::Use(0));
        assert_eq!(classify(&parts, 3, zero, Gesture::Tap, None), DragAction::Use(3));
        // Drag between slots: move.
        assert_eq!(classify(&parts, 0, five, Gesture::Drag, None), DragAction::Move(5));
        // Drag off the panel (world): drop. Same for a long-press drag.
        let world = Vec2::new(parts.frame.x - 200.0, parts.frame.centre().y);
        assert_eq!(classify(&parts, 2, world, Gesture::Drag, None), DragAction::Drop);
        // A visible trash chip wins even when it is drawn over the plate.
        let trash = parts.header;
        assert_eq!(
            classify(&parts, 2, trash.centre(), Gesture::Drag, Some(trash)),
            DragAction::Drop
        );
        // Without the chip the same point returns the item to its own slot.
        assert_eq!(
            classify(&parts, 2, trash.centre(), Gesture::Drag, None),
            DragAction::Move(2)
        );
        // A tap never drops, even outside the panel (a world tap is the caller's).
        assert_eq!(classify(&parts, 1, world, Gesture::Tap, None), DragAction::Use(1));
    }

    #[test]
    fn drag_from_slot_zero_to_slot_five_is_a_move() {
        let parts = plan(touch_panel_rect(100.0, 100.0, SCALE_L), SCALE_L);
        assert_eq!(drag_target(&parts, 0, parts.slots[5].centre()), DragAction::Move(5));
        assert_eq!(drag_target(&parts, 2, parts.slots[3].centre()), DragAction::Move(3));
        // Release on the origin slot is a tap, not a move.
        assert_eq!(drag_target(&parts, 4, parts.slots[4].centre()), DragAction::Use(4));
        // Release inside the plate but between slots keeps the item in place.
        assert_eq!(drag_target(&parts, 4, parts.header.centre()), DragAction::Move(4));
        // An out-of-range origin is a caller bug, not a panic.
        assert_eq!(drag_target(&parts, SLOTS, parts.slots[0].centre()), DragAction::Drop);
    }

    #[test]
    fn drag_outside_the_panel_drops() {
        let rect = touch_panel_rect(300.0, 300.0, SCALE_L);
        let parts = plan(rect, SCALE_L);
        for point in [
            Vec2::new(rect.x - 1.0, rect.centre().y),
            Vec2::new(rect.right() + 1.0, rect.centre().y),
            Vec2::new(rect.centre().x, rect.y - 1.0),
            Vec2::new(rect.centre().x, rect.bottom() + 1.0),
            Vec2::new(0.0, 0.0),
        ] {
            assert_eq!(drag_target(&parts, 1, point), DragAction::Drop, "release at {point:?}");
        }
    }

    #[test]
    fn zero_items_paints_the_frame_and_no_icons() {
        let skin = sample_skin();
        let rect = touch_panel_rect(200.0, 400.0, SCALE_L);

        let mut empty = UiBatch::default();
        let parts = draw(&skin, &mut empty, rect, &[], SCALE_L, [1.0; 4]);
        assert_eq!(parts.frame, rect);
        assert!(vertices(&empty) > 0, "the plate and the six frames are drawn");
        assert!(
            vertices(&empty) >= SLOTS * 6,
            "six slot frames at least one quad each: {}",
            vertices(&empty)
        );
        // Every vertex stays inside the plate.
        for draw in &empty.textured {
            for vertex in &draw.vertices {
                let (x, y) = (vertex.pos[0], vertex.pos[1]);
                assert!(
                    x >= rect.x - 0.5 && x <= rect.right() + 0.5 && y >= rect.y - 0.5
                        && y <= rect.bottom() + 0.5,
                    "vertex ({x}, {y}) outside {rect:?}"
                );
                assert!((0.0..=1.0).contains(&vertex.uv[0]));
                assert!((0.0..=1.0).contains(&vertex.uv[1]));
            }
        }

        // Occupancy changes the slot art and the cooldown veil, never an icon:
        // `ItemSlot::icon` is a name the caller resolves to its own texture, so
        // the same panel with six named icons and with six unnamed ones emits
        // exactly the same geometry.
        let mut named = UiBatch::default();
        let named_parts = draw(&skin, &mut named, rect, &occupied(), SCALE_L, [1.0; 4]);
        let mut unnamed = UiBatch::default();
        let anon: [Option<ItemSlot<'_>>; SLOTS] =
            std::array::from_fn(|index| Some(item_slot(None, (index + 1) as i32)));
        let anon_parts = draw(&skin, &mut unnamed, rect, &anon, SCALE_L, [1.0; 4]);
        assert_eq!(vertices(&named), vertices(&unnamed), "no icon quads in the batch");
        assert_eq!(named_parts, anon_parts, "icon names do not move geometry");
        assert_eq!(named_parts.icons, parts.icons, "occupied slots keep the icon window");
    }

    #[test]
    fn cooldown_veil_covers_only_the_icon_window() {
        let skin = sample_skin();
        let rect = touch_panel_rect(0.0, 0.0, SCALE_L);
        let mut batch = UiBatch::default();
        let ready = [Some(item_slot(Some("BTNAnkh"), 3)); SLOTS];
        draw(&skin, &mut batch, rect, &ready, SCALE_L, [1.0; 4]);
        let ready_quads = vertices(&batch);

        let mut batch = UiBatch::default();
        let cooling: [Option<ItemSlot<'_>>; SLOTS] = std::array::from_fn(|_| {
            Some(ItemSlot {
                icon: Some("BTNAnkh"),
                charges: 3,
                cooldown: 0.5,
            })
        });
        let parts = draw(&skin, &mut batch, rect, &cooling, SCALE_L, [1.0; 4]);
        assert_eq!(
            vertices(&batch),
            ready_quads + SLOTS * 6,
            "one veil quad per cooling slot"
        );
        // The last six vertices are the last slot's veil: the top half of its icon.
        let last = batch.textured.last().expect("veil emitted");
        let v = &last.vertices[last.vertices.len() - 6..];
        let icon = parts.icons[SLOTS - 1];
        assert!((v[0].pos[0] - icon.x).abs() < 0.5, "veil starts at the icon top");
        assert!((v[0].pos[1] - icon.y).abs() < 0.5);
        assert!((v[2].pos[1] - (icon.y + icon.h * 0.5)).abs() < 0.5, "veil is half height");
        // A NaN / out-of-range cooldown is treated as ready (no veil).
        let mut batch = UiBatch::default();
        let odd: [Option<ItemSlot<'_>>; SLOTS] = std::array::from_fn(|index| {
            Some(ItemSlot {
                icon: None,
                charges: 0,
                cooldown: if index == 0 { f32::NAN } else { -1.0 },
            })
        });
        draw(&skin, &mut batch, rect, &odd, SCALE_L, [1.0; 4]);
        assert_eq!(vertices(&batch), ready_quads, "NaN / negative cooldowns veil nothing");
    }

    #[test]
    fn slots_are_square_in_both_orientations() {
        for (scale, viewport) in [(SCALE_L, "2414x1080"), (SCALE_P, "1080x2414")] {
            let parts = plan(touch_panel_rect(10.0, 20.0, scale), scale);
            for (index, slot) in parts.slots.iter().enumerate() {
                assert!(
                    near(slot.w, slot.h, 1e-4),
                    "{viewport}: slot {index} {:?} is not square",
                    (slot.w, slot.h)
                );
                assert!(near(slot.w, TOUCH_MIN, 1e-3), "{viewport}: slot {index} side");
            }
            // Aspect independent of the panel aspect: a deliberately wide plate
            // still yields square slots (the grid centres itself).
            let wide = Rect::new(0.0, 0.0, 900.0, 200.0);
            for slot in plan(wide, scale).slots {
                assert!(near(slot.w, slot.h, 1e-4), "{viewport}: wide plate slot");
                assert!(slot.right() <= wide.right() + 1e-4);
                assert!(slot.bottom() <= wide.bottom() + 1e-4);
            }
            // ... and so does a tall one.
            let tall = Rect::new(0.0, 0.0, 200.0, 900.0);
            for slot in plan(tall, scale).slots {
                assert!(near(slot.w, slot.h, 1e-4), "{viewport}: tall plate slot");
                assert!(slot.right() <= tall.right() + 1e-4);
                assert!(slot.bottom() <= tall.bottom() + 1e-4);
            }
        }
    }

    #[test]
    fn panel_rect_is_the_exact_inverse_of_the_grid() {
        for (scale, cell) in [
            (SCALE_L, TOUCH_MIN),
            (SCALE_P, TOUCH_MIN),
            (1.1, 80.0),
            (2.0, 120.0),
            (0.62, 24.0),
        ] {
            let rect = panel_rect(0.0, 0.0, cell, scale);
            let parts = plan(rect, scale);
            for index in 0..SLOTS {
                assert!(
                    near(parts.slots[index].w, cell, 1e-3),
                    "scale {scale} cell {cell}: slot {} side {}",
                    index,
                    parts.slots[index].w
                );
            }
            // The grid fills the content box exactly on both axes.
            let first = parts.slots[0];
            let last = parts.slots[SLOTS - 1];
            let pad = PANEL_PAD * scale;
            assert!(near(first.x - rect.x, pad, 1e-3), "left gutter");
            assert!(near(rect.right() - last.right(), pad, 1e-3), "right gutter");
            assert!(near(rect.bottom() - last.bottom(), pad, 1e-3), "bottom gutter");
        }
    }

    #[test]
    fn degenerate_rects_do_not_poison_the_batch() {
        let skin = sample_skin();
        for rect in [
            Rect::new(0.0, 0.0, 0.0, 0.0),
            Rect::new(f32::NAN, 0.0, 10.0, 10.0),
            Rect::new(0.0, 0.0, 5.0, 5.0),
            Rect::new(0.0, 0.0, 40.0, 900.0),
        ] {
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, rect, &occupied(), SCALE_L, [1.0; 4]);
            for vertex in batch.textured.iter().flat_map(|draw| draw.vertices.iter()) {
                assert!(vertex.pos[0].is_finite() && vertex.pos[1].is_finite());
            }
            for slot in parts.slots {
                assert!(slot.w.is_finite() && slot.h.is_finite() && slot.w >= 0.0);
            }
            assert_eq!(hit(&parts, Vec2::new(-5.0, -5.0)), None);
        }
        // A NaN scale falls back to 1.0 instead of collapsing the padding.
        assert_eq!(art_scale(f32::NAN), 1.0);
        assert_eq!(art_scale(0.0), MIN_SCALE);
    }

    #[test]
    fn charge_label_shows_charged_items_only() {
        assert_eq!(charge_label(0), None, "unlimited item has no count");
        assert_eq!(charge_label(-3), None, "negative charges are not a count");
        assert_eq!(charge_label(1), Some("1".to_string()));
        assert_eq!(charge_label(12), Some("12".to_string()));
    }

    #[test]
    fn icon_basename_strips_path_and_extension() {
        assert_eq!(
            icon_basename("ReplaceableTextures\\CommandButtons\\BTNAnkh.blp").as_deref(),
            Some("BTNAnkh")
        );
        assert_eq!(
            icon_basename("ReplaceableTextures/CommandButtons/BTNAmulet").as_deref(),
            Some("BTNAmulet")
        );
        assert_eq!(icon_basename("   "), None);
        assert_eq!(icon_basename(""), None);
    }

    fn sim_item(def_index: u16) -> wc3_sim::Item {
        wc3_sim::Item {
            id: wc3_sim::ItemId(1),
            def_index,
            pos: Vec2::ZERO,
            owner: Some(0),
            charges: 1,
        }
    }

    #[test]
    fn icon_for_reads_the_installed_table() {
        // `desc` (Dagger of Escape) is a shipped item id; its `ITEMS` index is
        // what `Item::def_index` carries, so the "no icon" case reads that row
        // and the install goes to a different index on purpose.
        let unillustrated = wc3_sim::data::ITEMS
            .iter()
            .position(|def| def.id == "desc")
            .expect("shipped item id") as u16;
        let illustrated = if unillustrated == 0 { 1 } else { 0 };
        assert_eq!(
            install_icons([(illustrated, "ReplaceableTextures\\CommandButtons\\BTNAnkh.blp")]),
            1
        );
        assert!(icons_installed());
        assert_eq!(icon_for(&sim_item(illustrated)), Some("BTNAnkh"));
        assert_eq!(
            icon_for(&sim_item(unillustrated)),
            None,
            "an item the install has no icon for draws the grey placeholder"
        );
        // Case-insensitive interning: a second install reuses the same string.
        assert_eq!(install_icons([(illustrated, "btnankh")]), 1);
        assert_eq!(icon_for(&sim_item(illustrated)), Some("BTNAnkh"));
    }
}
