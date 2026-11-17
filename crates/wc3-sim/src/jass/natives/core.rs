//! Core natives: conversions, math, strings, players, hashtables, handle ids.
//!
//! Every handler mutates real state or computes a real value; nothing here
//! reports success for work it did not do. Natives whose behaviour depends on
//! hardware the headless simulation does not have (audio output, GPU) record
//! the request in the handle state so the query natives answer consistently.

use std::collections::BTreeMap;

use crate::jass::host::Object;
use crate::jass::value::{Handle, HandleKind, Value, format_real, real_to_int};
use crate::jass::vm::JassVm;

use super::{i, r, s};

// ---------------------------------------------------------------------------
// String / conversion
// ---------------------------------------------------------------------------

pub(super) fn i2r(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(i(a, 0) as f32)
}

pub(super) fn r2i(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(real_to_int(r(a, 0)))
}

pub(super) fn i2s(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Str(i(a, 0).to_string())
}

pub(super) fn r2s(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Str(format_real(r(a, 0)))
}

/// `R2SW(r, width, precision)` — fixed precision, padded to `width`.
pub(super) fn r2sw(_vm: &mut JassVm, a: &[Value]) -> Value {
    let value = r(a, 0);
    let width = i(a, 1).max(0) as usize;
    let precision = i(a, 2).max(0) as usize;
    let text = format!("{value:.precision$}");
    if text.len() < width {
        Value::Str(format!("{text:>width$}"))
    } else {
        Value::Str(text)
    }
}

pub(super) fn s2i(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(s(a, 0).trim().parse().unwrap_or(0))
}

pub(super) fn s2r(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(s(a, 0).trim().parse().unwrap_or(0.0))
}

/// `StringLength` counts bytes: the engine's strings are byte buffers and map
/// scripts index them with `SubString`, which is also byte based.
pub(super) fn string_length(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(s(a, 0).len() as i32)
}

/// `SubString(s, start, end)` — half-open byte range, clamped. The engine
/// tolerates `end < start` and out-of-range indices; both clamp to empty.
pub(super) fn substring(_vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 0);
    let bytes = text.as_bytes();
    let start = i(a, 1).clamp(0, bytes.len() as i32) as usize;
    let end = i(a, 2).clamp(0, bytes.len() as i32) as usize;
    if end <= start {
        return Value::Str(String::new());
    }
    match std::str::from_utf8(&bytes[start..end]) {
        Ok(slice) => Value::Str(slice.to_string()),
        // A slice that splits a multi-byte character yields the valid prefix.
        Err(_) => {
            let mut cut = end;
            while cut > start && !text.is_char_boundary(cut) {
                cut -= 1;
            }
            Value::Str(text[start..cut].to_string())
        }
    }
}

pub(super) fn string_case(_vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 0);
    if a.get(1).is_some_and(Value::truthy) {
        Value::Str(text.to_uppercase())
    } else {
        Value::Str(text.to_lowercase())
    }
}

/// `StringHash` — 32-bit FNV-1a. The engine's own hash is not observable in any
/// shipped map script (it exists for `StringHash`-based rawcode tricks, which
/// none of the 96 corpus scripts use), so a documented stable hash is used
/// rather than pretending to reproduce an unmeasured constant.
pub(super) fn string_hash(_vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 0);
    let mut hash: u32 = 0x811C_9DC5;
    for byte in text.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    Value::Int(hash as i32)
}

/// `GetHandleId` — the unique id of any handle, `0` for `null`.
pub(super) fn get_handle_id(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(a.first().and_then(Value::handle).map_or(0, |h| h.id as i32))
}

/// `AbilityId("AHea")` — a four-character ability id string to its rawcode.
pub(super) fn ability_id(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(fourcc(&s(a, 0)))
}

/// `AbilityId2String(id)` — rawcode back to its four characters.
pub(super) fn ability_id2string(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Str(fourcc_string(i(a, 0)))
}

// ---------------------------------------------------------------------------
// BJ player-slot natives (1.32 headers resolve `bj_MAX_PLAYERS` through these)
// ---------------------------------------------------------------------------

/// `GetBJMaxPlayers` — `bj_MAX_PLAYERS` in the 1.32 header.
pub(super) fn get_bj_max_players(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(crate::jass::host::PLAYER_SLOTS as i32 - 4)
}

/// `GetBJMaxPlayerSlots` — `bj_MAX_PLAYER_SLOTS`.
pub(super) fn get_bj_max_player_slots(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(crate::jass::host::PLAYER_SLOTS as i32)
}

/// `GetBJPlayerNeutralVictim`.
pub(super) fn get_bj_player_neutral_victim(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(crate::jass::host::PLAYER_NEUTRAL_VICTIM)
}

/// `GetBJPlayerNeutralExtra`.
pub(super) fn get_bj_player_neutral_extra(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(crate::jass::host::PLAYER_NEUTRAL_EXTRA)
}

/// `GetPlayerNeutralPassive`.
pub(super) fn get_player_neutral_passive(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(crate::jass::host::PLAYER_NEUTRAL_PASSIVE)
}

/// `GetPlayerNeutralAggressive`.
pub(super) fn get_player_neutral_aggressive(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(crate::jass::host::PLAYER_NEUTRAL_AGGRESSIVE)
}

// ---------------------------------------------------------------------------
// Math
// ---------------------------------------------------------------------------

pub(super) fn pow(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).powf(r(a, 1)))
}

pub(super) fn square_root(_vm: &mut JassVm, a: &[Value]) -> Value {
    let value = r(a, 0);
    Value::Real(if value < 0.0 { 0.0 } else { value.sqrt() })
}

pub(super) fn sin(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).sin())
}

pub(super) fn cos(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).cos())
}

pub(super) fn tan(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).tan())
}

pub(super) fn asin(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).clamp(-1.0, 1.0).asin())
}

pub(super) fn acos(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).clamp(-1.0, 1.0).acos())
}

pub(super) fn atan(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).atan())
}

pub(super) fn atan2(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0).atan2(r(a, 1)))
}

pub(super) fn deg2rad(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0) * std::f32::consts::PI / 180.0)
}

pub(super) fn rad2deg(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(r(a, 0) * 180.0 / std::f32::consts::PI)
}

pub(super) fn get_random_int(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(vm.random_int(i(a, 0), i(a, 1)))
}

pub(super) fn get_random_real(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(vm.random_real(r(a, 0), r(a, 1)))
}

pub(super) fn set_random_seed(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.rng = (i(a, 0) as u32 as u64) | 1;
    Value::Null
}

// ---------------------------------------------------------------------------
// Players
// ---------------------------------------------------------------------------

/// `Player(index)` — handle `#index` in `0..24`.
pub(super) fn player(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Handle(Handle::new(
        HandleKind::Player,
        i(a, 0).clamp(0, crate::jass::host::PLAYER_SLOTS as i32 - 1) as u32,
    ))
}

pub(super) fn get_player_id(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(player_index(a, 0).map_or(-1, |index| index as i32))
}

pub(super) fn get_local_player(vm: &mut JassVm, _a: &[Value]) -> Value {
    // The local slot is the one the map declared as `MAP_CONTROL_USER`;
    // Human01 puts the human in Player(1), and without this every
    // `if GetLocalPlayer() == player` block in Blizzard.j takes the wrong
    // branch (no camera, no transmission). Slot 0 is the fallback for maps
    // that never declare a controller.
    Value::Handle(Handle::new(HandleKind::Player, vm.host.local_player.unwrap_or(0) as u32))
}

pub(super) fn get_player_name(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Str(String::new());
    };
    Value::Str(
        vm.host
            .players
            .get(index as usize)
            .map(|state| state.name.clone())
            .unwrap_or_default(),
    )
}

pub(super) fn get_player_color(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.host
            .players
            .get(index as usize)
            .map_or(0, |state| state.color),
    )
}

pub(super) fn set_player_color(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state.color = i(a, 1);
    }
    Value::Null
}

pub(super) fn get_player_race(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.host
            .players
            .get(index as usize)
            .map_or(0, |state| state.race),
    )
}

pub(super) fn set_player_race_preference(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state.race = i(a, 1);
    }
    Value::Null
}

pub(super) fn set_player_race_selectable(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state.selecting = a.get(1).is_some_and(Value::truthy);
    }
    Value::Null
}

pub(super) fn set_player_controller(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        let control = i(a, 1);
        state.controller = control;
        // `MAP_CONTROL_USER` is 0 in `common.j` (`ConvertMapControl(0)`); the
        // engine also reports 2 from the old internal enum, so both mark the
        // slot as user-controlled.
        state.active = matches!(control, 0 | 1 | 2);
        if matches!(control, 0 | 2) && vm.host.local_player.is_none() {
            vm.host.local_player = Some(index as u8);
        }
    }
    Value::Null
}

pub(super) fn set_player_team(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state.team = i(a, 1);
    }
    Value::Null
}

pub(super) fn set_player_start_location(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state.start_location = i(a, 1);
    }
    Value::Null
}

pub(super) fn force_player_start_location(vm: &mut JassVm, a: &[Value]) -> Value {
    set_player_start_location(vm, a)
}

pub(super) fn get_player_start_location(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(-1);
    };
    Value::Int(
        vm.host
            .players
            .get(index as usize)
            .map_or(-1, |state| state.start_location),
    )
}

/// `SetPlayerAlliance(source, other, ALLIANCE_*, flag)`.
pub(super) fn set_player_alliance(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(from), Some(to)) = (player_index(a, 0), player_index(a, 1)) else {
        return Value::Null;
    };
    let kind = i(a, 2);
    let flag = a.get(3).is_some_and(Value::truthy);
    // Change detection compares the *effective* value on both seats, so
    // re-asserting the melee default is not a change and a real flip raises
    // `EVENT_PLAYER_ALLIANCE_CHANGED` for each side that observed it.
    let before = (
        vm.host.effective_alliance(from, to, kind),
        vm.host.effective_alliance(to, from, kind),
    );
    vm.host.alliances.insert(
        ((from as u32) << 8 | to as u32, kind),
        if flag { 1 } else { 0 },
    );
    let after = (
        vm.host.effective_alliance(from, to, kind),
        vm.host.effective_alliance(to, from, kind),
    );
    if before.0 != after.0 {
        vm.host.bump_alliance_seat(from, kind);
    }
    if to != from && before.1 != after.1 {
        vm.host.bump_alliance_seat(to, kind);
    }
    Value::Null
}

/// `GetPlayerAlliance(source, other, ALLIANCE_*)`. Defaults follow the engine's
/// melee rule: same team → allied, different team → not.
pub(super) fn get_player_alliance(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(from), Some(to)) = (player_index(a, 0), player_index(a, 1)) else {
        return Value::Bool(false);
    };
    let kind = i(a, 2);
    Value::Bool(vm.host.effective_alliance(from, to, kind))
}

pub(super) fn set_player_state(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        let key = i(a, 1);
        let value = i(a, 2);
        state.states.insert(key, value);
        // Gold and lumber are simulation resources for the two simulated
        // players; keep both views consistent.
        if (index as usize) < 2 {
            match key {
                k if k == player_state_gold() => vm.sim.resources[index as usize] = value,
                k if k == player_state_lumber() => vm.sim.lumber[index as usize] = value,
                _ => {}
            }
        }
    }
    Value::Null
}

pub(super) fn get_player_state(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(player_state_value(&vm.sim, &vm.host, index, i(a, 1)) as i32)
}

/// The player-state value both `GetPlayerState` and
/// `TriggerRegisterPlayerStateEvent` read, so a script can never see one number
/// and a trigger fire on another.
///
/// Gold/lumber/food/upkeep are simulation state for the two simulated seats
/// (`Sim` owns them); everything else is the script-side `PlayerState` map.
pub(crate) fn player_state_value(
    sim: &crate::Sim,
    host: &crate::jass::host::Host,
    player: u8,
    state: i32,
) -> f32 {
    if (player as usize) < 2 {
        let owner = player;
        match state {
            k if k == player_state_gold() => return sim.resources[owner as usize] as f32,
            k if k == player_state_lumber() => return sim.lumber[owner as usize] as f32,
            k if k == player_state_food_cap() => return sim.supply_cap(owner) as f32,
            k if k == player_state_food_used() => return sim.supply_used(owner) as f32,
            // `PLAYER_STATE_GOLD_UPKEEP_RATE` is the tax in percent (0/30/70),
            // while the simulation stores the income multiplier in permille
            // (1000/700/400), so 1000 - permille is the tax in tenths of a
            // percent.
            k if k == player_state_gold_upkeep_rate() => {
                return (1000 - sim.upkeep_permille(owner)) as f32 / 10.0;
            }
            k if k == player_state_lumber_upkeep_rate() => {
                return (1000 - sim.upkeep_permille(owner)) as f32 / 10.0;
            }
            _ => {}
        }
    }
    host.players
        .get(player as usize)
        .and_then(|seat| seat.states.get(&state).copied())
        .unwrap_or(0) as f32
}

/// Evaluates a `limitop` comparison (`LESS_THAN` … `NOT_EQUAL`).
pub(crate) fn compare_limit(current: f32, op: i32, value: f32) -> Option<bool> {
    match op {
        0 => Some(current < value),
        1 => Some(current <= value),
        2 => Some((current - value).abs() < 0.5),
        3 => Some(current >= value),
        4 => Some(current > value),
        5 => Some((current - value).abs() >= 0.5),
        _ => None,
    }
}

/// `PLAYER_STATE_RESOURCE_GOLD` = ConvertPlayerState(1).
fn player_state_gold() -> i32 {
    1
}

/// `PLAYER_STATE_RESOURCE_LUMBER` = ConvertPlayerState(2).
fn player_state_lumber() -> i32 {
    2
}

/// `PLAYER_STATE_RESOURCE_FOOD_CAP` = ConvertPlayerState(4).
fn player_state_food_cap() -> i32 {
    4
}

/// `PLAYER_STATE_RESOURCE_FOOD_USED` = ConvertPlayerState(5).
fn player_state_food_used() -> i32 {
    5
}

/// `PLAYER_STATE_GOLD_UPKEEP_RATE` = ConvertPlayerState(13).
fn player_state_gold_upkeep_rate() -> i32 {
    13
}

/// `PLAYER_STATE_LUMBER_UPKEEP_RATE` = ConvertPlayerState(14).
fn player_state_lumber_upkeep_rate() -> i32 {
    14
}

/// `GetLocalizedString(key)` — `war3map.wts` first, the install's
/// `GlobalStrings.fdf` second, the key itself (recorded as
/// `JASS_STRING missing=<key>`) when neither has it.
pub(super) fn get_localized_string(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Str(vm.host.strings.resolve(&s(a, 0)))
}

/// `IsMapFlagSet(flag)` — a `MAP_FLAG_*` bit of the loaded `war3map.w3i`
/// header. A value that is not an engine map-flag bit is answered `false` and
/// recorded once as `JASS_MAPFLAG unknown=<n>`; without a loaded header every
/// known flag is `false` (the header word is 0).
pub(super) fn is_map_flag_set(vm: &mut JassVm, a: &[Value]) -> Value {
    let flag = i(a, 0);
    match vm.host.map_flag(flag) {
        Some(set) => Value::Bool(set),
        None => {
            vm.host.note_unknown_map_flag(flag);
            Value::Bool(false)
        }
    }
}

pub(super) fn set_player_tech_researched(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state.tech_researched.insert(i(a, 1) as i64, i(a, 2));
    }
    Value::Null
}

pub(super) fn get_player_tech_researched(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.host
            .players
            .get(index as usize)
            .and_then(|state| state.tech_researched.get(&(i(a, 1) as i64)).copied())
            .unwrap_or(0),
    )
}

pub(super) fn set_player_tech_max_allowed(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state
            .tech_max
            .insert((i(a, 1) as i64, i(a, 2) as i64), i(a, 3));
    }
    Value::Null
}

pub(super) fn get_player_tech_max_allowed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(-1);
    };
    Value::Int(
        vm.host
            .players
            .get(index as usize)
            .and_then(|state| {
                state
                    .tech_max
                    .get(&(i(a, 1) as i64, i(a, 2) as i64))
                    .copied()
            })
            .unwrap_or(-1),
    )
}

pub(super) fn set_player_ability_available(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(index) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(index as usize)
    {
        state
            .ability_available
            .insert(i(a, 1) as i64, a.get(2).is_some_and(Value::truthy));
    }
    Value::Null
}

pub(super) fn get_player_unit_count(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    let include_incomplete = a.get(1).is_some_and(Value::truthy);
    let count = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.owner == index as u8)
        .filter(|unit| include_incomplete || unit.alive)
        .count();
    Value::Int(count as i32)
}

pub(super) fn get_player_structure_count(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    let count = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.owner == index as u8)
        .filter(|unit| unit.def().is_building())
        .count();
    Value::Int(count as i32)
}

/// `SetMapName`, `SetMapDescription`, `SetTeams`, `SetPlayers`,
/// `SetGamePlacement`, `SetGameTypeSupported`, `SetFloatGameState`: recorded
/// world state that the HUD/loader can read back.
pub(super) fn set_map_name(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.map_name = s(a, 0);
    Value::Null
}

pub(super) fn set_map_description(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.map_description = s(a, 0);
    Value::Null
}

pub(super) fn set_teams(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.teams = i(a, 0);
    Value::Null
}

pub(super) fn set_players(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.player_count = i(a, 0);
    Value::Null
}

pub(super) fn set_game_placement(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.placement = i(a, 0);
    Value::Null
}

pub(super) fn set_game_type_supported(vm: &mut JassVm, a: &[Value]) -> Value {
    let flag = a.get(1).is_some_and(Value::truthy);
    vm.host.game_types.insert(i(a, 0), flag);
    Value::Null
}

pub(super) fn get_game_type_selected(vm: &mut JassVm, _a: &[Value]) -> Value {
    // The first supported game type, like the lobby when only melee is offered.
    Value::Int(
        vm.host
            .game_types
            .iter()
            .find(|(_, flag)| **flag)
            .map_or(0, |(kind, _)| *kind),
    )
}

pub(super) fn set_float_game_state(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host
        .float_states
        .insert(i(a, 0), Value::Real(r(a, 1)));
    Value::Null
}

pub(super) fn get_float_game_state(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host
        .float_states
        .get(&i(a, 0))
        .cloned()
        .unwrap_or(Value::Real(0.0))
}

/// `BlzGetPlayerTownHallCount(player)` — the player's finished main halls,
/// which is what the melee victory check counts.
pub(super) fn blz_get_player_town_hall_count(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(player) = player_index(a, 0) else {
        return Value::Int(0);
    };
    let count = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == player)
        .filter(|unit| unit.is_building() && unit.def().is_main_hall())
        .count();
    Value::Int(count as i32)
}

/// `GetIntegerGameState(state)` — 0 means "no limit reached"; the simulation has
/// no victory-condition counters, so every game state reads 0.
pub(super) fn get_integer_game_state(vm: &mut JassVm, a: &[Value]) -> Value {
    let key = i(a, 0);
    Value::Int(vm.host.integer_game_states.get(&key).copied().unwrap_or(0))
}

/// `GetGameDifficulty` — melee default `GAME_DIFFICULTY_NORMAL` (2).
pub(super) fn get_game_difficulty(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(vm.host.game_difficulty)
}

pub(super) fn set_game_difficulty(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.game_difficulty = i(a, 0);
    Value::Null
}

// ---------------------------------------------------------------------------
// Query natives the melee BJ scripts need
// ---------------------------------------------------------------------------

/// `GetPlayerController(player)` — `MAP_CONTROL_USER`(2)/`COMPUTER`(1)/`NONE`(0).
pub(super) fn get_player_controller(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.host
            .players
            .get(index as usize)
            .map_or(0, |state| state.controller),
    )
}

/// `GetPlayerSlotState(player)` — 0 empty, 1 playing, 2 left.
pub(super) fn get_player_slot_state(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(index) = player_index(a, 0) else {
        return Value::Int(0);
    };
    let playing = vm
        .host
        .players
        .get(index as usize)
        .is_some_and(|state| state.active);
    Value::Int(if playing { 1 } else { 0 })
}

/// `IsPlayerObserver(player)` — the simulation has no observers.
pub(super) fn is_player_observer(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Bool(false)
}

/// `GetGameSpeed()` — `GAME_SPEED_NORMAL` (2) is the melee default.
pub(super) fn get_game_speed(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(2)
}

/// `VersionGet()` — `VERSION_FROZEN_THRONE` (1): all shipped maps are TFT.
pub(super) fn version_get(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(1)
}

/// `Preloader(path)` — the engine preloads assets; the headless simulation has
/// nothing to load, and the call is recorded as applied.
pub(super) fn preloader(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.preloaded.push(s(a, 0));
    Value::Null
}

/// `SetAllItemTypeSlots(maxSlots)` / `SetAllUnitTypeSlots(maxSlots)` — UI slot
/// caps, kept so a later query in the same script agrees.
pub(super) fn set_all_item_type_slots(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.all_item_type_slots = i(a, 0);
    Value::Null
}

pub(super) fn set_all_unit_type_slots(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.all_unit_type_slots = i(a, 0);
    Value::Null
}

/// `IsFogEnabled()` / `IsFogMaskEnabled(fogMask)` — no fog renderer exists.
pub(super) fn is_fog_enabled(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Bool(false)
}

/// `ForceEnumPlayers(force, filter)` — adds every slot that is playing and
/// passes the filter.
pub(super) fn force_enum_players(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(force) = a.first().and_then(Value::handle) else {
        return Value::Null;
    };
    let filter = a.get(1).and_then(Value::handle);
    let candidates: Vec<u8> = vm
        .host
        .players
        .iter()
        .filter(|state| state.active)
        .map(|state| state.index)
        .collect();
    let mut accepted = Vec::new();
    for player in candidates {
        let keep = match filter {
            None => true,
            Some(filter) => match vm.host.boolexpr(filter) {
                None => true,
                Some((None, _)) => true,
                Some((Some(func), negated)) => {
                    let saved = vm.enum_player;
                    vm.enum_player = Some(player);
                    let value = vm.call_script_function(func, Vec::new());
                    vm.enum_player = saved;
                    value.truthy() != negated
                }
            },
        };
        if keep {
            accepted.push(player);
        }
    }
    if let Some(Object::Force(members)) = vm.host.object_mut(force) {
        for player in accepted {
            if !members.contains(&player) {
                members.push(player);
            }
        }
        members.sort_unstable();
    }
    Value::Null
}

// ---------------------------------------------------------------------------
// Hashtables (1.24+ API used by everything newer than 1.24)
// ---------------------------------------------------------------------------

pub(super) fn init_hashtable(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Hashtable(BTreeMap::new())))
}

pub(super) fn flush_child_hashtable(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = a.first().and_then(Value::handle)
        && let Some(cells) = vm.host.hashtable_mut(handle)
    {
        let parent = i(a, 1) as i64;
        cells.retain(|(row, _), _| *row != parent);
    }
    Value::Null
}

pub(super) fn flush_parent_hashtable(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = a.first().and_then(Value::handle)
        && let Some(cells) = vm.host.hashtable_mut(handle)
    {
        let child = i(a, 1) as i64;
        cells.retain(|(_, column), _| *column != child);
    }
    Value::Null
}

pub(super) fn save_integer(vm: &mut JassVm, a: &[Value]) -> Value {
    save_cell(vm, a, Value::Int(i(a, 3)));
    Value::Null
}

pub(super) fn load_integer(vm: &mut JassVm, a: &[Value]) -> Value {
    match load_cell(vm, a) {
        Some(Value::Int(v)) => Value::Int(v),
        Some(Value::Real(v)) => Value::Int(real_to_int(v)),
        Some(Value::Bool(v)) => Value::Int(v as i32),
        Some(Value::Handle(h)) => Value::Int(h.id as i32),
        _ => Value::Int(0),
    }
}

pub(super) fn save_real(vm: &mut JassVm, a: &[Value]) -> Value {
    save_cell(vm, a, Value::Real(r(a, 3)));
    Value::Null
}

pub(super) fn load_real(vm: &mut JassVm, a: &[Value]) -> Value {
    match load_cell(vm, a) {
        Some(Value::Real(v)) => Value::Real(v),
        Some(Value::Int(v)) => Value::Real(v as f32),
        _ => Value::Real(0.0),
    }
}

pub(super) fn save_str(vm: &mut JassVm, a: &[Value]) -> Value {
    save_cell(vm, a, Value::Str(s(a, 3)));
    Value::Null
}

pub(super) fn load_str(vm: &mut JassVm, a: &[Value]) -> Value {
    match load_cell(vm, a) {
        Some(Value::Str(v)) => Value::Str(v),
        Some(other) => Value::Str(other.text()),
        None => Value::Str(String::new()),
    }
}

pub(super) fn save_boolean(vm: &mut JassVm, a: &[Value]) -> Value {
    save_cell(vm, a, Value::Bool(a.get(3).is_some_and(Value::truthy)));
    Value::Null
}

pub(super) fn load_boolean(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Bool(load_cell(vm, a).is_some_and(|value| value.truthy()))
}

pub(super) fn save_handle(vm: &mut JassVm, a: &[Value]) -> Value {
    save_cell(vm, a, a.get(3).cloned().unwrap_or(Value::Null));
    Value::Null
}

pub(super) fn load_handle(vm: &mut JassVm, a: &[Value]) -> Value {
    load_cell(vm, a).unwrap_or(Value::Null)
}

pub(super) fn have_saved_integer(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Bool(matches!(load_cell(vm, a), Some(Value::Int(_))))
}

pub(super) fn have_saved_real(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Bool(matches!(load_cell(vm, a), Some(Value::Real(_))))
}

pub(super) fn have_saved_str(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Bool(matches!(load_cell(vm, a), Some(Value::Str(_))))
}

pub(super) fn have_saved_boolean(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Bool(matches!(load_cell(vm, a), Some(Value::Bool(_))))
}

pub(super) fn have_saved_handle(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Bool(matches!(load_cell(vm, a), Some(Value::Handle(_))))
}

pub(super) fn remove_saved_integer(vm: &mut JassVm, a: &[Value]) -> Value {
    remove_cell(vm, a);
    Value::Null
}

pub(super) fn remove_saved_handle(vm: &mut JassVm, a: &[Value]) -> Value {
    remove_cell(vm, a);
    Value::Null
}

fn save_cell(vm: &mut JassVm, args: &[Value], value: Value) {
    let Some(handle) = args.first().and_then(Value::handle) else {
        return;
    };
    let key = (i(args, 1) as i64, i(args, 2) as i64);
    if let Some(cells) = vm.host.hashtable_mut(handle) {
        cells.insert(key, value);
    }
}

fn load_cell(vm: &mut JassVm, args: &[Value]) -> Option<Value> {
    let handle = args.first().and_then(Value::handle)?;
    let key = (i(args, 1) as i64, i(args, 2) as i64);
    vm.host.hashtable(handle)?.get(&key).cloned()
}

fn remove_cell(vm: &mut JassVm, args: &[Value]) {
    let Some(handle) = args.first().and_then(Value::handle) else {
        return;
    };
    let key = (i(args, 1) as i64, i(args, 2) as i64);
    if let Some(cells) = vm.host.hashtable_mut(handle) {
        cells.remove(&key);
    }
}

/// Player handle → slot index (`Player(i)` is `id = i`).
pub(super) fn player_index(args: &[Value], index: usize) -> Option<u8> {
    match args.get(index) {
        Some(Value::Handle(handle)) if handle.kind == HandleKind::Player => {
            Some(handle.id as u8)
        }
        _ => None,
    }
}

/// `'hfoo'` style four-character id to its integer rawcode.
pub(super) fn fourcc(text: &str) -> i32 {
    let bytes = text.as_bytes();
    if bytes.len() != 4 {
        return 0;
    }
    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Rawcode back to four characters (`0` → `"\0\0\0\0"`).
pub(super) fn fourcc_string(rawcode: i32) -> String {
    let bytes = rawcode.to_be_bytes();
    let text: String = bytes
        .iter()
        .map(|byte| {
            if *byte == 0 {
                '\0'
            } else {
                *byte as char
            }
        })
        .collect();
    text
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::jass::host::TriggerState;
    use crate::jass::natives::trigger::{
        trigger_register_player_alliance_change, trigger_register_player_state_event,
    };

    fn empty_vm() -> JassVm {
        let program = wc3_re::jass::ast::Program::default();
        JassVm::new(&program, crate::Sim::new(9, 16, 16), 9).expect("empty program")
    }

    fn player_handle(index: u8) -> Value {
        Value::Handle(Handle::new(HandleKind::Player, index as u32))
    }

    #[test]
    fn jass_natives_get_localized_string_never_returns_an_empty_string() {
        let mut vm = empty_vm();
        assert_eq!(vm.host.strings.load_wts("STRING 3\n{\nName\n}\n"), 1);
        let hit = get_localized_string(&mut vm, &[Value::Str("TRIGSTR_003".into())]);
        assert_eq!(hit.text(), "Name");
        let miss = get_localized_string(&mut vm, &[Value::Str("ABSENT".into())]);
        assert_eq!(miss.text(), "ABSENT", "an unresolved key comes back verbatim");
        assert_eq!(vm.host.strings.log, vec!["JASS_STRING missing=ABSENT"]);
        // Asking twice does not duplicate the log line.
        let _ = get_localized_string(&mut vm, &[Value::Str("ABSENT".into())]);
        assert_eq!(vm.host.strings.log.len(), 1);
    }

    #[test]
    fn jass_natives_is_map_flag_set_answers_from_the_header_word() {
        let mut vm = empty_vm();
        vm.host.map_flags = Some(0b101);
        assert!(is_map_flag_set(&mut vm, &[Value::Int(4)]).truthy(), "bit 2");
        assert!(!is_map_flag_set(&mut vm, &[Value::Int(8)]).truthy(), "bit 3");
        // 3 is not a single engine bit: false, and the gap is recorded.
        assert!(!is_map_flag_set(&mut vm, &[Value::Int(3)]).truthy());
        assert!(vm.host.unknown_map_flags.contains(&3));
        // Without a header every known flag is false (the word is 0).
        vm.host.map_flags = None;
        assert!(!is_map_flag_set(&mut vm, &[Value::Int(4)]).truthy());
    }

    #[test]
    fn jass_natives_trigger_register_player_state_event_seeds_the_edge_at_registration() {
        let mut vm = empty_vm();
        // PLAYER_STATE_ALLIED_VICTORY (8) already equals 1: the comparison holds
        // before the trigger exists, so the event must not fire on the first
        // tick (it fires on the crossing).
        vm.host.players[0].states.insert(8, 1);
        let trigger = vm.host.create(Object::Trigger(TriggerState::new()));
        let event = trigger_register_player_state_event(
            &mut vm,
            &[
                Value::Handle(trigger),
                player_handle(0),
                Value::Int(8),
                Value::Int(2),
                Value::Real(1.0),
            ],
        );
        assert!(matches!(event, Value::Handle(_)), "an event handle is returned");
        let key = (trigger.id, 0u8, 8, 2, 1.0f32.to_bits());
        assert_eq!(vm.host.state_watch.get(&key), Some(&true), "edge seeded");
        let state = vm.host.trigger(trigger).expect("trigger object");
        assert!(matches!(
            state.events.as_slice(),
            [crate::jass::host::RegEvent::PlayerStateEvent { player: 0, .. }]
        ));
    }

    #[test]
    fn jass_natives_trigger_register_player_alliance_change_seeds_the_epoch() {
        let mut vm = empty_vm();
        let trigger = vm.host.create(Object::Trigger(TriggerState::new()));
        let event = trigger_register_player_alliance_change(
            &mut vm,
            &[Value::Handle(trigger), player_handle(0), Value::Int(0)],
        );
        assert!(matches!(event, Value::Handle(_)));
        assert_eq!(vm.host.alliance_watch.get(&(trigger.id, 0, 0)), Some(&0));
    }
}
