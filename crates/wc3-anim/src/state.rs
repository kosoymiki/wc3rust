//! In-game animation state selection (the rule the engine must apply).
//!
//! `sync_visuals` used to derive the sequence from a coarse activity check with
//! two defects: walking units with a cooldown played `Attack` while moving, and
//! a building under construction played `Idle` instead of `Birth`. The priority
//! implemented here is the corrected one; `crates/wc3-game/src/game.rs` should
//! delegate to [`select_state`] (read-only patch in
//! `analysis/agent-anim/game_state_patch.md`).

use crate::GameplayState;

/// Coarse order classification the sim exposes to the visual layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityOrder {
    None,
    Move,
    Attack,
    Build,
    Other,
}

/// Everything the visual layer needs to pick a state for one unit/building.
#[derive(Debug, Clone, Copy)]
pub struct UnitActivity {
    pub building: bool,
    /// `construct.is_some()` — a building still under construction.
    pub constructing: bool,
    /// Model has no animation (particle/effect units) — state is still chosen.
    pub dead: bool,
    pub casting: bool,
    pub channeling: bool,
    pub order: ActivityOrder,
    pub path_empty: bool,
    /// Attack cooldown still running (does NOT mean "attack while walking").
    pub cooldown: bool,
    /// Movement type hover/fly: a model that authors no Walk (Wisp) uses its
    /// idle sequence while moving — the authored data has no ground gait.
    pub hover: bool,
}

impl Default for UnitActivity {
    fn default() -> Self {
        Self {
            building: false,
            constructing: false,
            dead: false,
            casting: false,
            channeling: false,
            order: ActivityOrder::None,
            path_empty: true,
            cooldown: false,
            hover: false,
        }
    }
}

/// Documented hover fallback: the model has no Walk sequence (Wisp) and the
/// engine keeps the idle/hover pose while moving. Returns `true` when the
/// engine should substitute the idle sequence for a moving hovering unit.
pub fn hover_uses_idle(activity: &UnitActivity) -> bool {
    activity.hover && select_state(activity) == GameplayState::Walk
}

/// Priority: dead > constructing (Birth) > channel > cast > building attack
/// > worker build > attack (standing only) > walk > idle.
///
/// Walking always wins over a running cooldown: a unit moving with cooldown
/// left must not freeze in its attack pose.
pub fn select_state(activity: &UnitActivity) -> GameplayState {
    if activity.dead {
        return GameplayState::Death;
    }
    if activity.constructing {
        return GameplayState::Birth;
    }
    if activity.channeling {
        return GameplayState::Channel;
    }
    if activity.casting {
        return GameplayState::Cast;
    }
    if activity.building {
        return if activity.order == ActivityOrder::Attack {
            GameplayState::Attack
        } else {
            GameplayState::Idle
        };
    }
    if activity.order == ActivityOrder::Build {
        return GameplayState::Build;
    }
    if activity.order == ActivityOrder::Attack && activity.path_empty {
        return GameplayState::Attack;
    }
    if !activity.path_empty || activity.order == ActivityOrder::Move {
        return GameplayState::Walk;
    }
    if activity.cooldown && activity.path_empty {
        return GameplayState::Attack;
    }
    GameplayState::Idle
}
