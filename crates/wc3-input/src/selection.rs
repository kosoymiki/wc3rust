//! Selection policy shared by the keyboard and touch paths (P0-10).
//!
//! The game owns `Sim`/`Camera`; this module owns the *rules* so the
//! double-tap type select (`ui.selection.double`), the Shift/long-press add
//! (`ui.selection.add`) and the control-group recall (`ui.hotkeys`) are
//! testable without a window. Units arrive as plain snapshots; `screen` is the
//! camera projection of the unit's ground position (`None` behind the camera).

use glam::Vec2;

use crate::ControlGroups;

/// One selectable unit as the game sees it for a single decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitSnapshot {
    pub id: u32,
    pub owner: u8,
    /// Compact unit kind (the game uses its `UnitKind as u32`; the policy only
    /// compares equality).
    pub kind: u32,
    pub pos: Vec2,
    /// Projected ground position; `None` when the camera rejected the point.
    pub screen: Option<Vec2>,
    pub alive: bool,
    /// Hero units form the first Tab subgroup (WC3 §ui.groups.tab).
    pub hero: bool,
    /// Buildings are never part of a box selection (WC3: dragging picks
    /// troops only; a structure is selected by a single tap).
    pub building: bool,
}

impl UnitSnapshot {
    pub fn new(id: u32, owner: u8, kind: u32, pos: Vec2) -> Self {
        Self {
            id,
            owner,
            kind,
            pos,
            screen: None,
            alive: true,
            hero: false,
            building: false,
        }
    }

    pub fn on_screen(mut self, screen: Vec2) -> Self {
        self.screen = Some(screen);
        self
    }

    pub fn dead(mut self) -> Self {
        self.alive = false;
        self
    }

    pub fn hero(mut self) -> Self {
        self.hero = true;
        self
    }

    /// Mark a structure: box selection skips it.
    pub fn building(mut self) -> Self {
        self.building = true;
        self
    }
}

/// How a pick combines with the existing selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Replace,
    Add,
}

/// WC3 double-click: every living unit of the same owner and kind whose
/// projected position is inside the viewport. Snapshot order is preserved so
/// the selection/panel order stays stable.
pub fn same_type_on_screen(
    units: &[UnitSnapshot],
    player: u8,
    kind: u32,
    viewport: Vec2,
) -> Vec<u32> {
    units
        .iter()
        .filter(|unit| unit.alive && unit.owner == player && unit.kind == kind)
        .filter(|unit| unit.screen.is_some_and(|screen| in_viewport(screen, viewport)))
        .map(|unit| unit.id)
        .collect()
}

fn in_viewport(point: Vec2, viewport: Vec2) -> bool {
    point.x >= 0.0 && point.y >= 0.0 && point.x < viewport.x && point.y < viewport.y
}

/// Box select in world space. `additive` (Shift) keeps the current selection
/// and appends the boxed units; duplicates are collapsed, dead units ignored.
pub fn box_selection(
    units: &[UnitSnapshot],
    player: u8,
    min: Vec2,
    max: Vec2,
    additive: bool,
    current: &[u32],
) -> Vec<u32> {
    let mut selection: Vec<u32> = if additive { current.to_vec() } else { Vec::new() };
    for unit in units.iter().filter(|unit| {
        unit.alive
            && unit.owner == player
            && !unit.building
            && unit.pos.x >= min.x
            && unit.pos.x <= max.x
            && unit.pos.y >= min.y
            && unit.pos.y <= max.y
    }) {
        if !selection.contains(&unit.id) {
            selection.push(unit.id);
        }
    }
    selection
}

/// Box select against the marquee the player actually drew, in screen space.
///
/// The two world points under a screen drag are not rectangle corners on the
/// ground: under the perspective camera the covered ground quad is a
/// trapezoid, so its axis-aligned world box is wider than the drawn marquee at
/// the far edge and over-selects units the player did not box. WC3 tests the
/// projection, so this is the rule for the marquee path; [`box_selection`]
/// stays for callers that only have world positions.
pub fn box_selection_screen(
    units: &[UnitSnapshot],
    player: u8,
    min: Vec2,
    max: Vec2,
    additive: bool,
    current: &[u32],
) -> Vec<u32> {
    let mut selection: Vec<u32> = if additive { current.to_vec() } else { Vec::new() };
    for unit in units.iter().filter(|unit| {
        unit.alive
            && unit.owner == player
            && !unit.building
            && unit.screen.is_some_and(|screen| {
                screen.x >= min.x && screen.x <= max.x && screen.y >= min.y && screen.y <= max.y
            })
    }) {
        if !selection.contains(&unit.id) {
            selection.push(unit.id);
        }
    }
    selection
}

/// Merge a single pick (tap, long-press) into the selection.
pub fn merge(current: &mut Vec<u32>, id: u32, mode: Mode) {    match mode {
        Mode::Replace => {
            current.clear();
            current.push(id);
        }
        Mode::Add => {
            if !current.contains(&id) {
                current.push(id);
            }
        }
    }
}

/// Control-group recall: the group's living units in stored order; `None` when
/// the slot is empty or every stored unit is gone (the game toasts GROUP GONE).
pub fn recall(groups: &ControlGroups, slot: u8, units: &[UnitSnapshot]) -> Option<Vec<u32>> {
    let stored = groups.recall(slot)?;
    let selection: Vec<u32> = stored
        .iter()
        .copied()
        .filter(|id| units.iter().any(|unit| unit.id == *id && unit.alive))
        .collect();
    if selection.is_empty() {
        None
    } else {
        Some(selection)
    }
}
