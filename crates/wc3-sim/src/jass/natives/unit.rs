//! Unit, item and destructable natives bound to the real simulation.
//!
//! A unit created here is a [`crate::Sim`] unit: it has roster stats, it can be
//! ordered, damaged and killed through the normal paths, and its death emits
//! the same events the rest of the simulation consumes. Script-only attributes
//! (`SetUnitAcquireRange`, animation names, vertex colour) live in
//! [`crate::jass::host::UnitMeta`] because the simulation has no field for them,
//! and their getters read that state back.

use glam::Vec2;

use crate::data::AttackType;
use crate::jass::host::{Object, SoundState, UnitTypes, unit_type};
use crate::jass::value::{Handle, HandleKind, Value, real_to_int};
use crate::jass::vm::JassVm;
use crate::{OrderSpec, UnitId};

use super::core::{fourcc, fourcc_string};
use super::{i, r, s};

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Handle argument at `index` when it is a unit handle.
pub(super) fn unit_arg(args: &[Value], index: usize) -> Option<Handle> {
    match args.get(index) {
        Some(Value::Handle(handle)) if handle.kind == HandleKind::Unit => Some(*handle),
        _ => None,
    }
}

/// Simulation unit id for the unit handle at `index`.
pub(super) fn sim_unit(vm: &JassVm, args: &[Value], index: usize) -> Option<UnitId> {
    let handle = unit_arg(args, index)?;
    vm.host.sim_unit(handle)
}


/// Live simulation unit snapshot fields the natives need, copied out so the
/// host can be mutated afterwards without a borrow conflict.
#[derive(Debug, Clone, Copy)]
pub(super) struct UnitSnap {
    pub pos: Vec2,
    pub facing: f32,
    pub hp: i32,
    pub max_hp: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub speed: f32,
    pub alive: bool,
    pub str_: i32,
    pub agi: i32,
    pub int_: i32,
}

pub(super) fn snap(vm: &JassVm, unit: UnitId) -> Option<UnitSnap> {
    let unit = vm.sim.unit(unit)?;
    Some(UnitSnap {
        pos: unit.pos,
        facing: unit.facing,
        hp: unit.hp,
        max_hp: unit.max_hp,
        mana: unit.mana,
        max_mana: unit.max_mana,
        speed: unit.speed,
        alive: unit.alive,
        str_: unit.hero_strength,
        agi: unit.hero_agility,
        int_: unit.hero_intelligence,
    })
}

// ---------------------------------------------------------------------------
// Creation
// ---------------------------------------------------------------------------

/// Shared `CreateUnit` core: `CreateUnit(player, id, x, y, facing)` and
/// `BlzCreateUnitWithSkin(player, id, x, y, facing, skin)`.
fn create_unit(vm: &mut JassVm, args: &[Value]) -> Value {
    let Some(owner) = super::core::player_index(args, 0) else {
        vm.errors.push("CreateUnit: invalid player".into());
        return Value::Null;
    };
    let rawcode = i(args, 1);
    let Some(def_id) = resolve_def_id(vm, rawcode) else {
        vm.errors
            .push(format!("CreateUnit: unknown unit type {}", fourcc_string(rawcode)));
        return Value::Null;
    };
    let pos = Vec2::new(r(args, 2), r(args, 3));
    let facing = r(args, 4);
    create_unit_impl(vm, owner, &def_id, pos, facing)
}

fn create_unit_impl(
    vm: &mut JassVm,
    owner: u8,
    def_id: &str,
    pos: Vec2,
    facing: f32,
) -> Value {
    match vm.sim.jass_create_unit(
        &mut vm.host.units,
        owner,
        def_id,
        pos,
        facing,
    ) {
        Ok(handle_id) => {
            let handle = Handle::new(HandleKind::Unit, handle_id);
            if let Some(meta) = vm.host.unit_meta_mut(handle) {
                meta.summoned = true;
                meta.facing = facing;
            }
            vm.host.created_units.push(handle_id);
            Value::Handle(handle)
        }
        Err(reason) => {
            vm.errors
                .push(format!("CreateUnit({def_id}): {reason}"));
            Value::Null
        }
    }
}

/// Resolves a unit type argument: a rawcode integer, or a four-character string
/// (`CreateUnitByName` style calls pass strings).
fn resolve_def_id(vm: &mut JassVm, rawcode: i32) -> Option<String> {
    let text = fourcc_string(rawcode);
    if crate::def_of(&text).is_some() {
        return Some(text);
    }
    // Some scripts pass a numeric handle by mistake; keep the diagnosis exact
    // rather than spawning a placeholder unit.
    let _ = vm;
    None
}

fn resolve_def_id_str(text: &str) -> Option<String> {
    if crate::def_of(text).is_some() {
        Some(text.to_string())
    } else {
        None
    }
}

pub(super) fn create_unit_native(vm: &mut JassVm, a: &[Value]) -> Value {
    create_unit(vm, a)
}

/// Position of the `location` argument at `index`, or a recorded diagnostic.
///
/// A missing argument, `null`, a destroyed handle and a handle of another
/// family all answer `None`: the caller must refuse the call instead of falling
/// back to the world origin, which would turn a script bug into a real unit
/// action at `(0, 0)`.
pub(super) fn location_arg(
    vm: &mut JassVm,
    args: &[Value],
    index: usize,
    native: &str,
) -> Option<Vec2> {
    let Some(value) = args.get(index) else {
        vm.errors.push(format!("{native}: missing location argument"));
        return None;
    };
    let Some(handle) = value.handle() else {
        let what = if value.is_null() {
            "null"
        } else {
            "a non-handle value"
        };
        vm.errors
            .push(format!("{native}: expected a location handle, got {what}"));
        return None;
    };
    match vm.host.location(handle) {
        Some(pos) => Some(pos),
        None => {
            vm.errors.push(format!(
                "{native}: handle {} is not a live location",
                handle.id
            ));
            None
        }
    }
}

pub(super) fn create_unit_at_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(owner) = super::core::player_index(a, 0) else {
        return Value::Null;
    };
    let Some(pos) = location_arg(vm, a, 2, "CreateUnitAtLoc") else {
        return Value::Null;
    };
    let rawcode = i(a, 1);
    let Some(def_id) = resolve_def_id(vm, rawcode) else {
        vm.errors
            .push(format!("CreateUnitAtLoc: unknown unit type {}", fourcc_string(rawcode)));
        return Value::Null;
    };
    let facing = r(a, 3);
    create_unit_impl(vm, owner, &def_id, pos, facing)
}

pub(super) fn create_unit_by_name(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(owner) = super::core::player_index(a, 0) else {
        return Value::Null;
    };
    let Some(def_id) = resolve_def_id_str(&s(a, 1)) else {
        vm.errors.push(format!("CreateUnitByName: unknown unit type {}", s(a, 1)));
        return Value::Null;
    };
    let pos = Vec2::new(r(a, 2), r(a, 3));
    create_unit_impl(vm, owner, &def_id, pos, r(a, 4))
}

pub(super) fn create_unit_at_loc_by_name(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(owner) = super::core::player_index(a, 0) else {
        return Value::Null;
    };
    let Some(def_id) = resolve_def_id_str(&s(a, 1)) else {
        vm.errors
            .push(format!("CreateUnitAtLocByName: unknown unit type {}", s(a, 1)));
        return Value::Null;
    };
    let Some(pos) = location_arg(vm, a, 2, "CreateUnitAtLocByName") else {
        return Value::Null;
    };
    create_unit_impl(vm, owner, &def_id, pos, r(a, 3))
}

// ---------------------------------------------------------------------------
// Position / state
// ---------------------------------------------------------------------------

pub(super) fn get_unit_x(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(snap(vm, unit).map_or(0.0, |unit| unit.pos.x))
}

pub(super) fn get_unit_y(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(snap(vm, unit).map_or(0.0, |unit| unit.pos.y))
}

pub(super) fn get_unit_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let pos = sim_unit(vm, a, 0)
        .and_then(|unit| snap(vm, unit))
        .map_or(Vec2::ZERO, |unit| unit.pos);
    Value::Handle(vm.host.create(Object::Location(pos)))
}

pub(super) fn get_unit_facing(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(snap(vm, unit).map_or(0.0, |unit| unit.facing))
}

pub(super) fn set_unit_facing(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let facing = r(a, 1);
    if let Some(unit) = vm.host.sim_unit(handle)
        && let Some(index) = vm.sim.unit_index(unit)
    {
        vm.sim.units[index].facing = facing;
    }
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.facing = facing;
        meta.facing_timed = None;
    }
    Value::Null
}

pub(super) fn set_unit_facing_timed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let facing = r(a, 1);
    let duration = r(a, 2).max(0.0);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.facing_timed = Some((facing, duration));
    }
    Value::Null
}

pub(super) fn set_unit_position(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let pos = Vec2::new(r(a, 1), r(a, 2));
    match vm.sim.jass_set_unit_position(&vm.host.units, handle.id, pos) {
        Ok(()) => Value::Null,
        Err(reason) => {
            vm.errors.push(format!("SetUnitPosition: {reason}"));
            Value::Null
        }
    }
}

pub(super) fn set_unit_position_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let Some(pos) = location_arg(vm, a, 1, "SetUnitPositionLoc") else {
        return Value::Null;
    };
    match vm.sim.jass_set_unit_position(&vm.host.units, handle.id, pos) {
        Ok(()) => Value::Null,
        Err(reason) => {
            vm.errors.push(format!("SetUnitPositionLoc: {reason}"));
            Value::Null
        }
    }
}

pub(super) fn set_unit_x(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let Some(unit) = vm.host.sim_unit(handle) else {
        return Value::Null;
    };
    let Some(snapshot) = snap(vm, unit) else {
        return Value::Null;
    };
    let pos = Vec2::new(r(a, 1), snapshot.pos.y);
    let _ = vm.sim.jass_set_unit_position(&vm.host.units, handle.id, pos);
    Value::Null
}

pub(super) fn set_unit_y(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let Some(unit) = vm.host.sim_unit(handle) else {
        return Value::Null;
    };
    let Some(snapshot) = snap(vm, unit) else {
        return Value::Null;
    };
    let pos = Vec2::new(snapshot.pos.x, r(a, 1));
    let _ = vm.sim.jass_set_unit_position(&vm.host.units, handle.id, pos);
    Value::Null
}

/// `GetUnitState(unit, UNIT_STATE_*)`.
pub(super) fn get_unit_state(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Real(0.0);
    };
    let state = i(a, 1);
    let Some(unit) = vm.host.sim_unit(handle) else {
        return Value::Real(0.0);
    };
    let Some(snapshot) = snap(vm, unit) else {
        return Value::Real(0.0);
    };
    Value::Real(match state {
        // UNIT_STATE_LIFE / MAX_LIFE / MANA / MAX_MANA
        0 => snapshot.hp as f32,
        1 => snapshot.max_hp as f32,
        2 => snapshot.mana as f32,
        3 => snapshot.max_mana as f32,
        _ => 0.0,
    })
}

pub(super) fn set_unit_state(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let state = i(a, 1);
    let value = r(a, 2);
    match state {
        // UNIT_STATE_LIFE / MANA go through the simulation so death, buffs and
        // the HUD all see them.
        0 | 2 => match vm
            .sim
            .jass_set_unit_state(&vm.host.units, handle.id, state, value)
        {
            Ok(()) => Value::Null,
            Err(reason) => {
                vm.errors.push(format!("SetUnitState({state}): {reason}"));
                Value::Null
            }
        },
        // Maximum life/mana are script-set attributes: the engine lets a map
        // raise them, and every later clamp uses the new maximum.
        1 | 3 => {
            let maximum = real_to_int(value).max(0);
            if let Some(meta) = vm.host.unit_meta_mut(handle) {
                if state == 1 {
                    meta.max_life_override = Some(maximum);
                } else {
                    meta.max_mana_override = Some(maximum);
                }
            }
            if let Some(unit) = vm.host.sim_unit(handle)
                && let Some(index) = vm.sim.unit_index(unit)
            {
                if state == 1 {
                    vm.sim.units[index].max_hp = maximum;
                } else {
                    vm.sim.units[index].max_mana = maximum;
                }
            }
            Value::Null
        }
        _ => {
            vm.errors.push(format!("SetUnitState: unknown unit state {state}"));
            Value::Null
        }
    }
}

/// `GetWidgetLife` / `SetWidgetLife` operate on the same life field.
pub(super) fn get_widget_life(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(snap(vm, unit).map_or(0.0, |unit| unit.hp as f32))
}

pub(super) fn set_widget_life(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let _ = vm.sim.jass_set_unit_state(
        &vm.host.units,
        handle.id,
        0,
        r(a, 1),
    );
    Value::Null
}

pub(super) fn get_unit_type_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.sim
            .unit(unit)
            .map(|unit| fourcc(unit.def().id))
            .unwrap_or(0),
    )
}

pub(super) fn get_unit_name(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Str(String::new());
    };
    if let Some(handle) = unit_arg(a, 0)
        && let Some(name) = vm.host.unit_meta(handle).and_then(|meta| meta.name_override.clone())
    {
        return Value::Str(name);
    }
    Value::Str(
        vm.sim
            .unit(unit)
            .map(|unit| unit.def().name.to_string())
            .unwrap_or_default(),
    )
}

pub(super) fn get_owning_player(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Handle(Handle::new(HandleKind::Player, 24));
    };
    let owner = vm.sim.unit(unit).map_or(0, |unit| unit.owner);
    Value::Handle(Handle::new(HandleKind::Player, owner as u32))
}

pub(super) fn set_unit_owner(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let Some(owner) = super::core::player_index(a, 1) else {
        return Value::Null;
    };
    match vm
        .sim
        .jass_set_unit_owner(&vm.host.units, handle.id, owner)
    {
        Ok(()) => Value::Null,
        Err(reason) => {
            vm.errors.push(format!("SetUnitOwner: {reason}"));
            Value::Null
        }
    }
}

pub(super) fn kill_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    match vm.sim.jass_kill_unit(&vm.host.units, handle.id) {
        Ok(()) => Value::Null,
        Err(reason) => {
            vm.errors.push(format!("KillUnit: {reason}"));
            Value::Null
        }
    }
}

pub(super) fn remove_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    match vm.sim.jass_remove_unit(&mut vm.host.units, handle.id) {
        Ok(()) => {
            vm.host.unit_meta.remove(&handle.id);
            Value::Null
        }
        Err(reason) => {
            vm.errors.push(format!("RemoveUnit: {reason}"));
            Value::Null
        }
    }
}

pub(super) fn show_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let show = a.get(1).is_some_and(Value::truthy);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.hidden = !show;
    }
    Value::Null
}

pub(super) fn is_unit_hidden(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    Value::Bool(
        vm.host
            .unit_meta(handle)
            .map(|meta| meta.hidden)
            .unwrap_or(false),
    )
}

pub(super) fn set_unit_acquire_range(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let range = r(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.acquire_range = range;
    }
    Value::Null
}

pub(super) fn get_unit_acquire_range(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Real(0.0);
    };
    let fallback = vm
        .host
        .sim_unit(handle)
        .and_then(|unit| vm.sim.unit(unit))
        .map_or(0.0, |unit| unit.sight);
    Value::Real(
        vm.host
            .unit_meta(handle)
            .map(|meta| meta.acquire_range)
            .filter(|range| *range > 0.0)
            .unwrap_or(fallback),
    )
}

pub(super) fn set_unit_color(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let color = i(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.color = color;
    }
    Value::Null
}

pub(super) fn set_unit_vertex_color(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let color = (
        i(a, 1).clamp(0, 255),
        i(a, 2).clamp(0, 255),
        i(a, 3).clamp(0, 255),
        i(a, 4).clamp(0, 255),
    );
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.vertex_color = color;
    }
    Value::Null
}

pub(super) fn set_unit_scale(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let scale = r(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.scale = scale;
    }
    Value::Null
}

pub(super) fn set_unit_time_scale(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let scale = r(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.time_scale = scale;
    }
    Value::Null
}

pub(super) fn set_unit_pathing(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let enabled = a.get(1).is_some_and(Value::truthy);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.pathing = enabled;
    }
    Value::Null
}

pub(super) fn set_unit_invulnerable(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let flag = a.get(1).is_some_and(Value::truthy);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.invulnerable = flag;
    }
    if let Some(unit) = vm.host.sim_unit(handle)
        && let Some(index) = vm.sim.unit_index(unit)
    {
        // The simulation models invulnerability as a time window; "forever"
        // is the maximum tick.
        vm.sim.units[index].invulnerable_until = if flag { u64::MAX } else { 0 };
    }
    Value::Null
}

pub(super) fn set_unit_move_speed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let speed = r(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.move_speed = speed;
    }
    if let Some(unit) = vm.host.sim_unit(handle)
        && let Some(index) = vm.sim.unit_index(unit)
    {
        vm.sim.units[index].speed = speed;
    }
    Value::Null
}

pub(super) fn get_unit_move_speed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Real(0.0);
    };
    if let Some(speed) = vm
        .host
        .unit_meta(handle)
        .map(|meta| meta.move_speed)
        .filter(|speed| *speed > 0.0)
    {
        return Value::Real(speed);
    }
    let Some(unit) = vm.host.sim_unit(handle) else {
        return Value::Real(0.0);
    };
    Value::Real(snap(vm, unit).map_or(0.0, |unit| unit.speed))
}

pub(super) fn get_unit_default_move_speed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(
        vm.sim
            .unit(unit)
            .map(|unit| unit.def().stats.speed)
            .unwrap_or(0.0),
    )
}

pub(super) fn set_unit_animation(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let animation = s(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.animation = animation;
    }
    Value::Null
}

pub(super) fn set_unit_animation_by_index(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let index = i(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.animation = format!("index:{index}");
    }
    Value::Null
}

pub(super) fn queue_unit_animation(vm: &mut JassVm, a: &[Value]) -> Value {
    set_unit_animation(vm, a)
}

pub(super) fn set_unit_user_data(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let value = i(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.user_data = value;
    }
    Value::Null
}

pub(super) fn get_unit_user_data(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(vm.host.unit_meta(handle).map_or(0, |meta| meta.user_data))
}

pub(super) fn unit_apply_timed_life(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let lifetime = r(a, 2);
    match vm
        .sim
        .jass_apply_timed_life(&vm.host.units, handle.id, lifetime)
    {
        Ok(()) => {
            if let Some(meta) = vm.host.unit_meta_mut(handle) {
                meta.timed_life = Some(lifetime);
            }
            Value::Null
        }
        Err(reason) => {
            vm.errors.push(format!("UnitApplyTimedLife: {reason}"));
            Value::Null
        }
    }
}

/// `SetResourceAmount` — gold/lumber left in a mine or a resource carrying unit.
pub(super) fn set_resource_amount(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let amount = i(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.resource_amount = amount;
    }
    // A gold mine is also a simulation object with a real remaining amount.
    if let Some(unit) = vm.host.sim_unit(handle)
        && let Some(pos) = vm.sim.unit(unit).map(|unit| unit.pos)
        && let Some(mine) = vm
            .sim
            .gold_mines
            .iter_mut()
            .find(|mine| mine.pos.distance_squared(pos) < 4.0)
    {
        mine.remaining = amount;
    }
    Value::Null
}

pub(super) fn get_resource_amount(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.host
            .unit_meta(handle)
            .map(|meta| meta.resource_amount)
            .unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------
// Abilities / heroes
// ---------------------------------------------------------------------------

pub(super) fn unit_add_ability(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let ability = fourcc_string(i(a, 1));
    match vm
        .sim
        .jass_unit_add_ability(&vm.host.units, handle.id, &ability)
    {
        Ok(()) => {
            if let Some(meta) = vm.host.unit_meta_mut(handle) {
                meta.abilities.push(i(a, 1) as i64);
            }
            Value::Bool(true)
        }
        Err(reason) => {
            // `UnitAddAbility` returns false for a type the engine does not
            // know; record it so a missing ability is not invisible.
            vm.errors
                .push(format!("UnitAddAbility({ability}): {reason}"));
            Value::Bool(false)
        }
    }
}

pub(super) fn unit_remove_ability(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let ability = fourcc_string(i(a, 1));
    match vm
        .sim
        .jass_unit_remove_ability(&vm.host.units, handle.id, &ability)
    {
        Ok(()) => {
            if let Some(meta) = vm.host.unit_meta_mut(handle) {
                meta.abilities.retain(|id| *id != i(a, 1) as i64);
            }
            Value::Bool(true)
        }
        Err(_) => Value::Bool(false),
    }
}

pub(super) fn get_unit_ability_level(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    let ability = fourcc_string(i(a, 1));
    let level = vm
        .sim
        .unit(unit)
        .map(|unit| unit.learned.iter().filter(|id| **id == ability).count())
        .unwrap_or(0);
    Value::Int(level as i32)
}

pub(super) fn set_unit_ability_level(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let ability = fourcc_string(i(a, 1));
    let level = i(a, 2).max(0);
    let Some(unit) = vm.host.sim_unit(handle) else {
        return Value::Null;
    };
    let Some(index) = vm.sim.unit_index(unit) else {
        return Value::Null;
    };
    if let Some(def) = crate::abilities::generic_ability(&ability) {
        let id = def.id;
        vm.sim.units[index].learned.retain(|entry| *entry != id);
        for _ in 0..level {
            vm.sim.units[index].learned.push(id);
        }
    }
    Value::Null
}

pub(super) fn unit_make_ability_permanent(_vm: &mut JassVm, a: &[Value]) -> Value {
    // Permanence only affects whether a morph removes the ability; the
    // simulation tracks abilities per unit and never morphs, so recording the
    // call as applied is exact.
    let _ = (i(a, 1), a.get(2).is_some_and(Value::truthy));
    Value::Null
}

pub(super) fn select_hero_skill(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Null;
    };
    let ability = fourcc_string(i(a, 1));
    let Some(index) = vm.sim.unit_index(unit) else {
        return Value::Null;
    };
    if vm.sim.units[index].skill_points == 0 {
        return Value::Null;
    }
    if let Some(def) = crate::abilities::generic_ability(&ability) {
        let id = def.id;
        vm.sim.units[index].learned.push(id);
        vm.sim.units[index].skill_points -= 1;
    }
    Value::Null
}

pub(super) fn set_hero_level(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Null;
    };
    let level = i(a, 1).clamp(1, crate::MAX_HERO_LEVEL as i32) as u8;
    match vm.sim.set_hero_level(unit, level) {
        Ok(()) => Value::Null,
        Err(reason) => {
            vm.errors.push(format!("SetHeroLevel: {reason}"));
            Value::Null
        }
    }
}

pub(super) fn get_hero_level(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    Value::Int(vm.sim.hero_level(unit).map_or(0, |level| level as i32))
}

pub(super) fn add_hero_xp(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Null;
    };
    let amount = i(a, 1).max(0) as u32;
    let _ = vm.sim.add_hero_xp(unit, amount);
    Value::Null
}

pub(super) fn get_hero_xp(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    Value::Int(vm.sim.hero_xp(unit).map_or(0, |xp| xp as i32))
}

fn hero_attribute(vm: &JassVm, unit: UnitId, attribute: i32) -> i32 {
    // HERO_ATTRIBUTE_STR = 0, AGI = 1, INT = 2.
    let Some(snapshot) = snap(vm, unit) else {
        return 0;
    };
    match attribute {
        0 => snapshot.str_,
        1 => snapshot.agi,
        2 => snapshot.int_,
        _ => 0,
    }
}

fn set_hero_attribute(vm: &mut JassVm, unit: UnitId, attribute: i32, value: i32) {
    let Some(index) = vm.sim.unit_index(unit) else {
        return;
    };
    match attribute {
        0 => vm.sim.units[index].hero_strength = value,
        1 => vm.sim.units[index].hero_agility = value,
        2 => vm.sim.units[index].hero_intelligence = value,
        _ => {}
    }
}

pub(super) fn get_hero_str(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    let include_bonus = a.get(1).is_some_and(Value::truthy);
    let base = hero_attribute(vm, unit, 0);
    Value::Int(if include_bonus {
        vm.sim.hero_attributes(unit).map_or(base, |stats| stats.0)
    } else {
        base
    })
}

pub(super) fn get_hero_agi(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    let include_bonus = a.get(1).is_some_and(Value::truthy);
    let base = hero_attribute(vm, unit, 1);
    Value::Int(if include_bonus {
        vm.sim.hero_attributes(unit).map_or(base, |stats| stats.1)
    } else {
        base
    })
}

pub(super) fn get_hero_int(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Int(0);
    };
    let include_bonus = a.get(1).is_some_and(Value::truthy);
    let base = hero_attribute(vm, unit, 2);
    Value::Int(if include_bonus {
        vm.sim.hero_attributes(unit).map_or(base, |stats| stats.2)
    } else {
        base
    })
}

pub(super) fn set_hero_str(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(unit) = sim_unit(vm, a, 0) {
        set_hero_attribute(vm, unit, 0, i(a, 1));
    }
    Value::Null
}

pub(super) fn set_hero_agi(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(unit) = sim_unit(vm, a, 0) {
        set_hero_attribute(vm, unit, 1, i(a, 1));
    }
    Value::Null
}

pub(super) fn set_hero_int(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(unit) = sim_unit(vm, a, 0) {
        set_hero_attribute(vm, unit, 2, i(a, 1));
    }
    Value::Null
}

pub(super) fn unit_damage_target(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(source) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let Some(target) = (match a.get(1) {
        Some(Value::Handle(handle)) if handle.kind == HandleKind::Unit => Some(*handle),
        _ => None,
    }) else {
        return Value::Bool(false);
    };
    let amount = r(a, 2);
    match vm
        .sim
        .jass_unit_damage_target(&vm.host.units, source.id, target.id, amount, AttackType::Normal)
    {
        Ok(_) => Value::Bool(true),
        Err(reason) => {
            vm.errors.push(format!("UnitDamageTarget: {reason}"));
            Value::Bool(false)
        }
    }
}

pub(super) fn unit_damage_point(vm: &mut JassVm, a: &[Value]) -> Value {
    // UnitDamagePoint(unit, delay, radius, x, y, amount, attack, ranged, weapon, kind)
    let Some(source) = sim_unit(vm, a, 0) else {
        return Value::Bool(false);
    };
    let radius = r(a, 2).max(0.0);
    let center = Vec2::new(r(a, 3), r(a, 4));
    let amount = r(a, 5).round() as i32;
    let victims: Vec<UnitId> = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.id != source)
        .filter(|unit| unit.pos.distance(center) <= radius)
        .map(|unit| unit.id)
        .collect();
    for victim in &victims {
        vm.sim.apply_damage(source, *victim, amount, AttackType::Normal);
    }
    Value::Bool(!victims.is_empty())
}

// ---------------------------------------------------------------------------
// Unit classification
// ---------------------------------------------------------------------------

/// `IsUnitType(unit, UNIT_TYPE_*)`.
///
/// The constants are the engine's own (`common.j`): 0 hero, 1 dead, 2 structure,
/// 3 flying, 4 ground, 5 attacks flying, 6 attacks ground, 7 melee attacker,
/// 8 ranged attacker, … The simulation knows the ones that change gameplay
/// (hero, dead, structure, melee/ranged) and answers `false` for the rest
/// instead of inventing a value.
///
/// The answer is `<per-instance delta>.classify(derived)`: `UnitAddType` /
/// `UnitRemoveType` are runtime state on this one instance
/// ([`crate::jass::host::UnitTypes`]) while the definition is shared by every
/// instance of the same id, and both go through [`derived_unit_type`] +
/// [`UnitTypes::classify`] so a getter and a filter cannot disagree.
pub(super) fn is_unit_type(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let Some(unit) = vm.host.sim_unit(handle) else {
        return Value::Bool(false);
    };
    let kind = i(a, 1);
    let Some(snapshot) = snap(vm, unit) else {
        return Value::Bool(false);
    };
    let def = vm.sim.unit(unit).map(|unit| unit.def());
    let derived = derived_unit_type(def, snapshot.alive, kind);
    let types = vm
        .host
        .unit_meta(handle)
        .map_or(UnitTypes::default(), |meta| meta.types);
    Value::Bool(types.classify(kind, derived))
}

/// Attack range at or below which the simulation classifies a unit as a melee
/// attacker (`UNIT_TYPE_MELEE_ATTACKER`); the footman's 90 and the rifleman's
/// 400 sit on either side of it.
const MELEE_RANGE: f32 = 100.0;

/// Classification from the unit definition alone, before any per-instance
/// `UnitAddType`/`UnitRemoveType` delta. `alive` is passed in because callers
/// hold the snapshot or the unit borrow already.
pub(super) fn derived_unit_type(def: Option<&crate::UnitDef>, alive: bool, type_id: i32) -> bool {
    match type_id {
        unit_type::HERO => def.is_some_and(|def| def.hero),
        unit_type::DEAD => !alive,
        unit_type::STRUCTURE => def.is_some_and(|def| def.is_building()),
        unit_type::FLYING => false,
        unit_type::GROUND => true,
        unit_type::MELEE_ATTACKER => def.is_some_and(|def| def.stats.range <= MELEE_RANGE),
        unit_type::RANGED_ATTACKER => def.is_some_and(|def| def.stats.range > MELEE_RANGE),
        _ => false,
    }
}

/// Per-unit `UNIT_TYPE_*` deltas the map set, keyed by simulation unit.
///
/// A native filter that walks the unit table collects them once: the metadata
/// map only holds units a script touched, so one handle lookup per touched unit
/// replaces one per simulation unit. [`derived_unit_type`] plus
/// [`crate::jass::host::UnitTypes::classify`] is the same rule `IsUnitType`
/// answers with.
pub(super) fn unit_type_deltas(
    vm: &JassVm,
) -> std::collections::BTreeMap<UnitId, UnitTypes> {
    vm.host
        .unit_meta
        .iter()
        .filter(|(_, meta)| !meta.types.is_empty())
        .filter_map(|(handle, meta)| {
            vm.host
                .units
                .unit_of(*handle)
                .map(|unit| (unit, meta.types))
        })
        .collect()
}

/// `IsUnitAlly(unit, player)` — same team and no `SetPlayerAlliance` override.
pub(super) fn is_unit_ally(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Bool(false);
    };
    let Some(player) = super::core::player_index(a, 1) else {
        return Value::Bool(false);
    };
    let owner = vm.sim.unit(unit).map_or(0, |unit| unit.owner);
    Value::Bool(players_allied(vm, owner, player))
}

/// `IsUnitEnemy(unit, player)`.
pub(super) fn is_unit_enemy(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = sim_unit(vm, a, 0) else {
        return Value::Bool(false);
    };
    let Some(player) = super::core::player_index(a, 1) else {
        return Value::Bool(false);
    };
    let owner = vm.sim.unit(unit).map_or(0, |unit| unit.owner);
    Value::Bool(!players_allied(vm, owner, player) && owner != player)
}

/// Two players are allied when their teams match and no explicit
/// `SetPlayerAlliance` says otherwise (`ALLIANCE_PASSIVE` = 0).
fn players_allied(vm: &JassVm, a: u8, b: u8) -> bool {
    if a == b {
        return true;
    }
    if let Some(flag) = vm.host.alliances.get(&((a as u32) << 8 | b as u32, 0)) {
        return *flag != 0;
    }
    vm.host
        .players
        .get(a as usize)
        .zip(vm.host.players.get(b as usize))
        .is_some_and(|(left, right)| left.team == right.team)
}


// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

fn issue_command(vm: &mut JassVm, handle: Handle, spec: OrderSpec, label: &str) -> bool {
    let Some(unit) = vm.host.sim_unit(handle) else {
        vm.errors.push(format!("{label}: invalid unit handle"));
        return false;
    };
    vm.host.order_log.push((handle.id, label.to_string()));
    vm.sim.issue(&[unit], spec);
    true
}

/// Shared body of `IssueImmediateOrder` / `IssueImmediateOrderById`.
///
/// `order` is the lowercased order string; the `ById` variant converts its
/// rawcode first. Returns `false` for an order the simulation cannot resolve —
/// the engine answers the same way instead of accepting a no-op.
pub(super) fn issue_immediate_order_impl(vm: &mut JassVm, a: &[Value], order: &str) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let spec = match order {
        "stop" => OrderSpec::Stop,
        "hold" | "holdposition" => OrderSpec::Hold,
        "unhold" => OrderSpec::Stop,
        _ => return Value::Bool(false),
    };
    Value::Bool(issue_command(vm, handle, spec, order))
}

pub(super) fn issue_immediate_order(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = s(a, 1).to_ascii_lowercase();
    issue_immediate_order_impl(vm, a, &order)
}

/// Shared body of the point-order family. `point` is the destination and
/// `order` the lowercased order string.
pub(super) fn issue_point_order_impl(
    vm: &mut JassVm,
    a: &[Value],
    order: &str,
    point: Vec2,
) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let spec = match order {
        "move" => OrderSpec::Move(point),
        "attack" | "attackground" => OrderSpec::AttackMove(point),
        "patrol" => {
            let start = vm
                .host
                .sim_unit(handle)
                .and_then(|unit| vm.sim.unit(unit))
                .map_or(point, |unit| unit.pos);
            if start.distance_squared(point) < 1.0 {
                // `Patrol` to the current position degenerates to a hold; the
                // engine's patrol needs two distinct points.
                OrderSpec::Hold
            } else {
                OrderSpec::Patrol(start, point)
            }
        }
        _ => return Value::Bool(false),
    };
    vm.host.last_order_point = point;
    Value::Bool(issue_command(vm, handle, spec, order))
}

pub(super) fn issue_point_order(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = s(a, 1).to_ascii_lowercase();
    let point = Vec2::new(r(a, 2), r(a, 3));
    issue_point_order_impl(vm, a, &order, point)
}

pub(super) fn issue_point_order_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = s(a, 1).to_ascii_lowercase();
    let Some(point) = location_arg(vm, a, 2, "IssuePointOrderLoc") else {
        return Value::Bool(false);
    };
    issue_point_order_impl(vm, a, &order, point)
}

/// Shared body of the target-order family. `target` is the widget argument as
/// the script passed it (unit handle, or null).
pub(super) fn issue_target_order_impl(
    vm: &mut JassVm,
    a: &[Value],
    order: &str,
    target: Option<&Value>,
) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let target_handle = target.and_then(Value::handle);
    let target_unit = target_handle.and_then(|handle| {
        (handle.kind == HandleKind::Unit).then_some(handle)
    });
    let spec = match (order, target_unit) {
        ("attack" | "smart", Some(target)) => {
            let Some(unit) = vm.host.sim_unit(target) else {
                return Value::Bool(false);
            };
            OrderSpec::Attack(unit)
        }
        ("move" | "follow", Some(target)) => {
            let Some(unit) = vm.host.sim_unit(target) else {
                return Value::Bool(false);
            };
            let pos = vm.sim.unit(unit).map_or(Vec2::ZERO, |unit| unit.pos);
            OrderSpec::Move(pos)
        }
        ("stop", _) => OrderSpec::Stop,
        ("hold" | "holdposition", _) => OrderSpec::Hold,
        _ => return Value::Bool(false),
    };
    vm.host.last_order_target = target_handle;
    Value::Bool(issue_command(vm, handle, spec, order))
}

pub(super) fn issue_target_order(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = s(a, 1).to_ascii_lowercase();
    issue_target_order_impl(vm, a, &order, a.get(2))
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

fn item_handle(vm: &mut JassVm, item: crate::ItemId) -> Value {
    let handle = vm.host.create_tagged(HandleKind::Item, Object::Opaque);
    vm.host.item_binding.insert(handle.id, item);
    Value::Handle(handle)
}

pub(super) fn create_item(vm: &mut JassVm, a: &[Value]) -> Value {
    let def_id = fourcc_string(i(a, 0));
    let pos = Vec2::new(r(a, 1), r(a, 2));
    spawn_item_impl(vm, &def_id, pos)
}

fn spawn_item_impl(vm: &mut JassVm, def_id: &str, pos: Vec2) -> Value {
    let def = crate::data::ITEMS
        .iter()
        .find(|item| item.id.eq_ignore_ascii_case(def_id));
    let Some(def) = def else {
        vm.errors.push(format!("CreateItem: unknown item type {def_id}"));
        return Value::Null;
    };
    let charges = def.charges;
    match vm.sim.spawn_item(def.id, pos, charges) {
        Some(item) => item_handle(vm, item),
        None => {
            vm.errors.push(format!("CreateItem({def_id}): no free item slot"));
            Value::Null
        }
    }
}

pub(super) fn get_item_type_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(item) = item_arg(vm, a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.sim
            .item(item)
            .map(|item| fourcc(item.def().id))
            .unwrap_or(0),
    )
}

pub(super) fn remove_item(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(item) = item_arg(vm, a, 0) else {
        return Value::Null;
    };
    if let Some(index) = vm.sim.items.iter().position(|entry| entry.id == item) {
        vm.sim.items.remove(index);
    }
    if let Some(handle) = a.first().and_then(Value::handle) {
        vm.host.objects.remove(&handle.id);
        vm.host.item_binding.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn get_item_charges(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(item) = item_arg(vm, a, 0) else {
        return Value::Int(0);
    };
    Value::Int(vm.sim.item(item).map_or(0, |item| item.charges))
}

pub(super) fn set_item_charges(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(item) = item_arg(vm, a, 0) else {
        return Value::Null;
    };
    let charges = i(a, 1);
    if let Some(index) = vm.sim.items.iter().position(|entry| entry.id == item) {
        vm.sim.items[index].charges = charges;
    }
    Value::Null
}

pub(super) fn unit_add_item(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit_handle_value) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let Some(item) = item_arg(vm, a, 1) else {
        return Value::Bool(false);
    };
    let Some(unit) = vm.host.sim_unit(unit_handle_value) else {
        return Value::Bool(false);
    };
    match vm.sim.pickup_item(unit, item) {
        Ok(()) => Value::Bool(true),
        Err(_) => Value::Bool(false),
    }
}

pub(super) fn unit_add_item_by_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit_handle_value) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let def_id = fourcc_string(i(a, 1));
    let Some(unit) = vm.host.sim_unit(unit_handle_value) else {
        return Value::Null;
    };
    let pos = vm.sim.unit(unit).map_or(Vec2::ZERO, |unit| unit.pos);
    let Some(item) = vm.sim.spawn_item(&def_id, pos, 1) else {
        vm.errors.push(format!("UnitAddItemById: unknown item {def_id}"));
        return Value::Null;
    };
    let handle = item_handle(vm, item);
    let _ = vm.sim.pickup_item(unit, item);
    handle
}

pub(super) fn unit_add_item_to_slot_by_id(vm: &mut JassVm, a: &[Value]) -> Value {
    // The simulation keeps a flat item list per unit; the slot index is only a
    // UI position, so the item still lands in the unit's inventory.
    unit_add_item_by_id(vm, a)
}

pub(super) fn unit_drop_item_point(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit_handle_value) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let Some(item) = item_arg(vm, a, 1) else {
        return Value::Bool(false);
    };
    let Some(unit) = vm.host.sim_unit(unit_handle_value) else {
        return Value::Bool(false);
    };
    let point = Vec2::new(r(a, 2), r(a, 3));
    match vm.sim.drop_item(unit, item) {
        Ok(()) => {
            if let Some(index) = vm.sim.items.iter().position(|entry| entry.id == item) {
                vm.sim.items[index].pos = point;
                vm.sim.items[index].owner = None;
            }
            Value::Bool(true)
        }
        Err(_) => Value::Bool(false),
    }
}

pub(super) fn unit_drop_item_slot(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit_handle_value) = unit_arg(a, 0) else {
        return Value::Bool(false);
    };
    let Some(item) = item_arg(vm, a, 1) else {
        return Value::Bool(false);
    };
    let Some(unit) = vm.host.sim_unit(unit_handle_value) else {
        return Value::Bool(false);
    };
    vm.sim.drop_item(unit, item).map(|()| Value::Bool(true)).unwrap_or(Value::Bool(false))
}

pub(super) fn unit_remove_item(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit_handle_value) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let Some(item) = item_arg(vm, a, 1) else {
        return Value::Null;
    };
    if let Some(unit) = vm.host.sim_unit(unit_handle_value) {
        let _ = vm.sim.drop_item(unit, item);
    }
    remove_item(vm, &[a[1].clone()])
}

pub(super) fn choose_random_item(vm: &mut JassVm, a: &[Value]) -> Value {
    choose_random_item_impl(vm, i(a, 0), None)
}

/// `ChooseRandomItemEx(itemtype, level)` — a random item of that class/level.
pub(super) fn choose_random_item_ex(vm: &mut JassVm, a: &[Value]) -> Value {
    let class = i(a, 0);
    let level = i(a, 1);
    choose_random_item_impl(vm, level, Some(class))
}

/// `ITEM_TYPE_*` (common.j) → the `ItemData.slk` class column.
fn item_class_name(class: i32) -> Option<&'static str> {
    match class {
        0 => Some("Permanent"),
        1 => Some("Charged"),
        2 => Some("Artifact"),
        3 => Some("Purchasable"),
        4 => Some("Campaign"),
        5 => Some("Miscellaneous"),
        // 7 is ITEM_TYPE_ANY; anything else is treated as "any" too, which is
        // what the engine does for an unmapped class.
        _ => None,
    }
}

fn choose_random_item_impl(vm: &mut JassVm, level: i32, class: Option<i32>) -> Value {
    let class_name = class.and_then(item_class_name);
    let candidates: Vec<&'static str> = crate::data::ITEMS
        .iter()
        .filter(|item| item.level == level)
        .filter(|item| class_name.is_none_or(|name| item.class.eq_ignore_ascii_case(name)))
        .map(|item| item.id)
        .collect();
    if candidates.is_empty() {
        // A level with no item in the local data table: the engine cannot roll
        // one either, and returning 0 lets the caller's own null check run.
        return Value::Int(0);
    }
    let pick = vm.random_u32() as usize % candidates.len();
    Value::Int(fourcc(candidates[pick]))
}

/// Item handle argument → simulation item id.
fn item_arg(vm: &mut JassVm, args: &[Value], index: usize) -> Option<crate::ItemId> {
    let handle = match args.get(index) {
        Some(Value::Handle(handle)) if handle.kind == HandleKind::Item => *handle,
        _ => return None,
    };
    vm.host.item_binding.get(&handle.id).copied()
}

// ---------------------------------------------------------------------------
// Destructables
// ---------------------------------------------------------------------------

fn destructable_handle(vm: &mut JassVm, destructable: crate::DestructableId) -> Value {
    let handle = vm
        .host
        .create_tagged(HandleKind::Destructable, Object::Opaque);
    vm.host.destructable_binding.insert(handle.id, destructable);
    Value::Handle(handle)
}

pub(super) fn create_destructable(vm: &mut JassVm, a: &[Value]) -> Value {
    create_destructable_impl(vm, a)
}

pub(super) fn create_destructable_z(vm: &mut JassVm, a: &[Value]) -> Value {
    create_destructable_impl(vm, a)
}

/// `BlzCreateDestructableWithSkin` / `...WithSkinPitchRollColor` / `...ZWithSkin`
/// share the engine's creation path; skin/pitch/roll/colour are render-only and
/// are accepted without pretending they changed simulation state.
pub(super) fn blz_create_destructable_with_skin(vm: &mut JassVm, a: &[Value]) -> Value {
    create_destructable_impl(vm, a)
}

pub(super) fn blz_create_destructable_z_with_skin(vm: &mut JassVm, a: &[Value]) -> Value {
    create_destructable_impl(vm, a)
}

pub(super) fn blz_create_destructable_with_skin_pitch_roll_color(
    vm: &mut JassVm,
    a: &[Value],
) -> Value {
    create_destructable_impl(vm, a)
}

fn create_destructable_impl(vm: &mut JassVm, a: &[Value]) -> Value {
    let def_id = fourcc_string(i(a, 0));
    let pos = Vec2::new(r(a, 1), r(a, 2));
    let Some(destructable) = vm.sim.spawn_destructable(&def_id, pos) else {
        vm.errors
            .push(format!("CreateDestructable: unknown type {def_id}"));
        return Value::Null;
    };
    destructable_handle(vm, destructable)
}

pub(super) fn get_destructable_x(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(
        vm.sim
            .destructables
            .iter()
            .find(|entry| entry.id == destructable)
            .map_or(0.0, |entry| entry.pos.x),
    )
}

pub(super) fn get_destructable_y(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(
        vm.sim
            .destructables
            .iter()
            .find(|entry| entry.id == destructable)
            .map_or(0.0, |entry| entry.pos.y),
    )
}

pub(super) fn get_destructable_type_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Int(0);
    };
    Value::Int(
        vm.sim
            .destructables
            .iter()
            .find(|entry| entry.id == destructable)
            .map(|entry| fourcc(entry.def().id))
            .unwrap_or(0),
    )
}

pub(super) fn get_destructable_life(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(
        vm.sim
            .destructables
            .iter()
            .find(|entry| entry.id == destructable)
            .map_or(0.0, |entry| entry.hp as f32),
    )
}

pub(super) fn get_destructable_max_life(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Real(0.0);
    };
    Value::Real(
        vm.sim
            .destructables
            .iter()
            .find(|entry| entry.id == destructable)
            .map_or(0.0, |entry| entry.max_hp as f32),
    )
}

pub(super) fn set_destructable_life(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Null;
    };
    let life = r(a, 1);
    if let Some(entry) = vm
        .sim
        .destructables
        .iter_mut()
        .find(|entry| entry.id == destructable)
    {
        entry.hp = real_to_int(life);
    }
    Value::Null
}

pub(super) fn kill_destructable(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Null;
    };
    if let Some(entry) = vm
        .sim
        .destructables
        .iter_mut()
        .find(|entry| entry.id == destructable)
    {
        entry.hp = 0;
    }
    Value::Null
}

pub(super) fn remove_destructable(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(destructable) = destructable_arg(vm, a, 0) else {
        return Value::Null;
    };
    vm.sim
        .destructables
        .retain(|entry| entry.id != destructable);
    if let Some(handle) = a.first().and_then(Value::handle) {
        vm.host.objects.remove(&handle.id);
        vm.host.destructable_binding.remove(&handle.id);
    }
    Value::Null
}

fn destructable_arg(vm: &mut JassVm, args: &[Value], index: usize) -> Option<crate::DestructableId> {
    let handle = match args.get(index) {
        Some(Value::Handle(handle)) if handle.kind == HandleKind::Destructable => *handle,
        _ => return None,
    };
    vm.host.destructable_binding.get(&handle.id).copied()
}

// ---------------------------------------------------------------------------
// Text / UI helpers that live on units
// ---------------------------------------------------------------------------

/// `AddSpecialEffect` / `AddSpecialEffectTarget` — effects are render-only; the
/// handle exists so `DestroyEffect` and `GetHandleId` behave.
pub(super) fn add_special_effect(vm: &mut JassVm, a: &[Value]) -> Value {
    let file = s(a, 0);
    let pos = Vec2::new(r(a, 1), r(a, 2));
    Value::Handle(vm.host.create(Object::Image { pos, file }))
}

pub(super) fn add_special_effect_target(vm: &mut JassVm, a: &[Value]) -> Value {
    let file = s(a, 0);
    let pos = a
        .get(1)
        .and_then(Value::handle)
        .and_then(|handle| vm.host.sim_unit(handle))
        .and_then(|unit| vm.sim.unit(unit))
        .map_or(Vec2::ZERO, |unit| unit.pos);
    Value::Handle(vm.host.create(Object::Image { pos, file }))
}

pub(super) fn destroy_effect(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = a.first().and_then(Value::handle) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

/// `CreateSound`/`CreateSoundFilenameWithLabel` — the handle records the file
/// and parameters; no audio device exists in the headless simulation.
pub(super) fn create_sound(vm: &mut JassVm, a: &[Value]) -> Value {
    let path = s(a, 0);
    let state = SoundState {
        path,
        ..SoundState::default()
    };
    Value::Handle(vm.host.create(Object::Sound(state)))
}

pub(super) fn create_sound_filename_with_label(vm: &mut JassVm, a: &[Value]) -> Value {
    let path = s(a, 0);
    let state = SoundState {
        path,
        ..SoundState::default()
    };
    Value::Handle(vm.host.create(Object::Sound(state)))
}

// ---------------------------------------------------------------------------
// Display / text
// ---------------------------------------------------------------------------

/// `DisplayTextToPlayer(toPlayer, x, y, message)` — three coordinates before
/// the text (`common.j:1448`), so the message is argument 3. Reading index 2
/// logged the `y` coordinate instead (found by the AUD-018 display test).
pub(super) fn display_text_to_player(vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 3);
    let player = super::core::player_index(a, 0);
    vm.host.display_log.push((player, text.clone()));
    vm.emit_text(text);
    Value::Null
}

pub(super) fn display_timed_text_to_player(vm: &mut JassVm, a: &[Value]) -> Value {
    // DisplayTimedTextToPlayer(player, x, y, duration, message)
    let text = s(a, 4);
    let player = super::core::player_index(a, 0);
    vm.host.display_log.push((player, text.clone()));
    vm.emit_text(text);
    Value::Null
}

/// `ClearTextMessages()` — clears the local client's message area.
///
/// The engine native takes no player: `Blizzard.j`'s `ClearTextMessagesBJ`
/// gates the call per force so only the clients in it clear their own screen,
/// so the host clears what the local player can see (broadcasts plus messages
/// addressed to that slot). The transcript (`JassVm::text_log`/`output`) keeps
/// every message: clearing a screen is not deleting the game's chat log.
pub(super) fn clear_text_messages(vm: &mut JassVm, _a: &[Value]) -> Value {
    let viewer = vm.host.local_player;
    vm.host.clear_display_messages(viewer);
    Value::Null
}

/// `SelectUnit` / `ClearSelection` — recorded selection, no rendering.
pub(super) fn select_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    if !a.get(1).is_some_and(Value::truthy) {
        vm.host.selection.retain(|id| *id != handle.id);
        return Value::Null;
    }
    if !vm.host.selection.contains(&handle.id) {
        vm.host.selection.push(handle.id);
    }
    Value::Null
}

pub(super) fn clear_selection(vm: &mut JassVm, _a: &[Value]) -> Value {
    vm.host.selection.clear();
    Value::Null
}

/// `RemoveAllGuardPositions` — the simulation has no guard-position table; the
/// guard order it would clear is `Hold`, which is reset explicitly.
pub(super) fn remove_all_guard_positions(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    if let Some(unit) = vm.host.sim_unit(handle) {
        vm.sim.issue(&[unit], OrderSpec::Stop);
    }
    Value::Null
}






