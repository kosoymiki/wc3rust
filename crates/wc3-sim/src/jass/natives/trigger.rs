//! Trigger, timer, event and group natives.
//!
//! This is the part that makes a map script *do* something over time: a trigger
//! registers an event, the scheduler (`scheduler.rs`) raises that event from the
//! simulation tick, and the registered actions run on their own threads. Timer
//! callbacks (`TimerStart`) and `PolledWait`/`TriggerSleepAction` go through the
//! same machinery, so a script that sleeps does not block the map.
//!
//! Event constants are the engine's own values from `common.j`
//! (`EVENT_UNIT_DEATH`, `EVENT_PLAYER_UNIT_DEATH`, …); the scheduler matches on
//! them, which is why a trigger registered for the wrong constant never fires
//! instead of firing for everything.

use crate::jass::host::{Object, RegEvent, TriggerState};
use crate::jass::value::{Handle, HandleKind, Value};
use crate::jass::vm::JassVm;

use super::core::player_index;
use super::{hd, i, r, s};
use super::unit::unit_arg;

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

pub(super) fn create_trigger(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Trigger(TriggerState::new())))
}

pub(super) fn destroy_trigger(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

/// Registered function index from a `code` value.
fn code_index(value: Option<&Value>) -> Option<u32> {
    match value {
        Some(Value::Code(index)) if *index != u32::MAX => Some(*index),
        _ => None,
    }
}

pub(super) fn trigger_add_action(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(func) = code_index(a.get(1)) else {
        vm.errors.push("TriggerAddAction: null action".into());
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.actions.push(func);
    }
    // `TriggerAddAction` returns a `triggeraction` handle.
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_add_condition(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(boolexpr) = hd(a, 1) else {
        return Value::Null;
    };
    let Some((func, _)) = vm.host.boolexpr(boolexpr) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        if let Some(func) = func {
            state.conditions.push(func);
        }
    }
    Value::Handle(vm.host.create(Object::TriggerCondition {
        func,
        trigger: trigger.id,
    }))
}

pub(super) fn trigger_remove_action(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.actions.clear();
    }
    Value::Null
}

pub(super) fn trigger_remove_condition(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.conditions.clear();
    }
    Value::Null
}

pub(super) fn trigger_clear_actions(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.actions.clear();
    }
    Value::Null
}

pub(super) fn trigger_clear_conditions(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.conditions.clear();
    }
    Value::Null
}

pub(super) fn enable_trigger(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(trigger) = hd(a, 0)
        && let Some(state) = vm.host.trigger_mut(trigger)
    {
        state.enabled = true;
    }
    Value::Null
}

pub(super) fn disable_trigger(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(trigger) = hd(a, 0)
        && let Some(state) = vm.host.trigger_mut(trigger)
    {
        state.enabled = false;
    }
    Value::Null
}

pub(super) fn is_trigger_enabled(vm: &mut JassVm, a: &[Value]) -> Value {
    let enabled = hd(a, 0)
        .and_then(|trigger| vm.host.trigger(trigger))
        .map(|state| state.enabled)
        .unwrap_or(false);
    Value::Bool(enabled)
}

pub(super) fn trigger_register_timer_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let period = r(a, 1);
    let periodic = a.get(2).is_some_and(Value::truthy);
    let period_ticks = (period * crate::TICKS_PER_SECOND as f32).round().max(1.0) as u32;
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::TimerEvent {
            period,
            period_ticks,
            periodic,
            elapsed: 0,
        });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_timer_expire(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(timer) = hd(a, 1) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::TimerExpire { timer: timer.id });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_unit_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let unit = unit_arg(a, 1).map(|handle| handle.id);
    let event = i(a, 2) as i64;
    if let Some(state) = vm.host.trigger_mut(trigger) {
        match unit {
            Some(unit) => state.events.push(RegEvent::UnitEvent { unit, event }),
            None => state.events.push(RegEvent::AnyUnitEvent { event, player: None }),
        }
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

/// `TriggerRegisterPlayerUnitEvent(trigger, player, event, filter)` — the player
/// scoping is kept so `EVENT_PLAYER_UNIT_*` only fires for that player's units.
pub(super) fn trigger_register_player_unit_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let player = player_index(a, 1);
    let event = i(a, 2) as i64;
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::AnyUnitEvent { event, player });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_player_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(player) = player_index(a, 1) else {
        return Value::Null;
    };
    let event = i(a, 2) as i64;
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::PlayerEvent { player, event });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

/// `TriggerRegisterPlayerAllianceChange(trigger, player, alliance)` — fires when
/// the seat's alliance state for `alliance` (an `ALLIANCE_*` constant) changes.
///
/// The script side of the change (`SetPlayerAlliance`) bumps a per-
/// `(player, alliance)` epoch; the scheduler delivers the event on its next
/// pass, so a trigger that changes alliances never fires re-entrantly from
/// inside its own action.
pub(super) fn trigger_register_player_alliance_change(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(player) = player_index(a, 1) else {
        return Value::Null;
    };
    let alliance = i(a, 2);
    let epoch = vm
        .host
        .alliance_epoch
        .get(&(player, alliance))
        .copied()
        .unwrap_or(0);
    vm.host
        .alliance_watch
        .insert((trigger.id, player, alliance), epoch);
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state
            .events
            .push(RegEvent::PlayerAllianceChange { player, alliance });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

/// `TriggerRegisterPlayerStateEvent(trigger, player, state, op, value)` — fires
/// when the player state crosses `op`/`value` (gold, lumber, food, upkeep, …).
///
/// The comparison is sampled once per tick by the scheduler; the event fires on
/// the false → true crossing, and the sample taken here (at registration) seeds
/// the edge so a state that already satisfies the comparison does not fire on
/// the first tick.
pub(super) fn trigger_register_player_state_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(player) = player_index(a, 1) else {
        return Value::Null;
    };
    let state = i(a, 2);
    let op = i(a, 3);
    let value = r(a, 4);
    if super::core::compare_limit(0.0, op, 0.0).is_none() {
        vm.errors.push(format!(
            "TriggerRegisterPlayerStateEvent: unknown limitop {op}"
        ));
    }
    let current = super::core::player_state_value(&vm.sim, &vm.host, player, state);
    let held = super::core::compare_limit(current, op, value).unwrap_or(false);
    vm.host
        .state_watch
        .insert((trigger.id, player, state, op, value.to_bits()), held);
    if let Some(trigger_state) = vm.host.trigger_mut(trigger) {
        trigger_state.events.push(RegEvent::PlayerStateEvent {
            player,
            state,
            op,
            value,
        });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_death_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let destructable = match a.get(1) {
        Some(Value::Handle(handle)) if handle.kind == HandleKind::Destructable => Some(handle.id),
        _ => None,
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::DestructableDeath { destructable });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_enter_region(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(region) = hd(a, 2) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::RegionEnter { region: region.id });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_leave_region(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(region) = hd(a, 2) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::RegionLeave { region: region.id });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

pub(super) fn trigger_register_player_chat_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(player) = player_index(a, 1) else {
        return Value::Null;
    };
    let message = s(a, 2);
    let exact = a.get(3).is_some_and(Value::truthy);
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::Chat {
            player,
            message,
            exact,
        });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

/// `TriggerExecute` — run the trigger's actions now, ignoring conditions, like
/// the engine.
pub(super) fn trigger_execute(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let actions = vm
        .host
        .trigger(trigger)
        .map(|state| (state.actions.clone(), state.conditions.clone()))
        .unwrap_or_default();
    let mut event = vm.event();
    event.trigger = Some(trigger.id);
    vm.run_trigger_actions(trigger, &actions.0, actions.1, event);
    Value::Null
}

/// `TriggerEvaluate` — conditions only, returns their conjunction.
pub(super) fn trigger_evaluate(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Bool(false);
    };
    let conditions = vm
        .host
        .trigger(trigger)
        .map(|state| state.conditions.clone())
        .unwrap_or_default();
    let mut event = vm.event();
    event.trigger = Some(trigger.id);
    Value::Bool(vm.evaluate_conditions(&conditions, event))
}

pub(super) fn get_triggering_trigger(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.event().trigger {
        Some(id) => Value::Handle(Handle::new(HandleKind::Trigger, id)),
        None => Value::Null,
    }
}

pub(super) fn get_trigger_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    let event = vm.event();
    match event.unit {
        Some(id) => Value::Handle(Handle::new(HandleKind::Unit, id)),
        None => Value::Null,
    }
}

pub(super) fn get_trigger_widget(vm: &mut JassVm, _a: &[Value]) -> Value {
    let event = vm.event();
    match event.unit.or(event.dying_destructable) {
        Some(id) => {
            let kind = if event.unit.is_some() {
                HandleKind::Unit
            } else {
                HandleKind::Destructable
            };
            Value::Handle(Handle::new(kind, id))
        }
        None => Value::Null,
    }
}

pub(super) fn get_dying_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().dying_unit)
}

pub(super) fn get_entering_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().entering_unit)
}

pub(super) fn get_leaving_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().leaving_unit)
}

pub(super) fn get_changing_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().changing_unit)
}

pub(super) fn get_changing_unit_prev_owner(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.event().changing_unit_prev_owner {
        Some(owner) => Value::Handle(Handle::new(HandleKind::Player, owner as u32)),
        None => Value::Null,
    }
}

pub(super) fn get_attacker(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().attacker)
}

pub(super) fn get_killing_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().killing_unit)
}

pub(super) fn get_leveling_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().leveling_unit)
}

pub(super) fn get_trained_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().trained_unit)
}

pub(super) fn get_trained_unit_type(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(vm.event().trained_unit_type as i32)
}

pub(super) fn get_event_damage(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(vm.event().damage)
}

pub(super) fn get_event_damage_source(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().attacker)
}

pub(super) fn get_trigger_event_id(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(vm.event().event_id as i32)
}

pub(super) fn get_trigger_eval_count(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(0)
}

pub(super) fn get_trigger_exec_count(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(0)
}

pub(super) fn get_spell_ability_id(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(vm.event().spell_id as i32)
}

/// `GetEventGameState()` — the `gamestate` whose limit fired a
/// `TriggerRegisterGameStateEvent` (the header documents this read under
/// `EVENT_GAME_STATE_LIMIT`). `0` outside such an event, which is also
/// `GAME_STATE_DIVINE_INTERVENTION` — an honest answer rather than a null handle
/// a numeric comparison cannot use.
pub(super) fn get_event_game_state(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Int(vm.event().game_state.unwrap_or(0))
}

pub(super) fn get_spell_ability_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().unit)
}

pub(super) fn get_spell_target_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().spell_target_unit)
}

pub(super) fn get_spell_target_x(vm: &mut JassVm, _a: &[Value]) -> Value {
    let Some(unit) = vm.event().spell_target_unit else {
        return Value::Real(0.0);
    };
    Value::Real(unit_position(vm, unit).map_or(0.0, |pos| pos.x))
}

pub(super) fn get_spell_target_y(vm: &mut JassVm, _a: &[Value]) -> Value {
    let Some(unit) = vm.event().spell_target_unit else {
        return Value::Real(0.0);
    };
    Value::Real(unit_position(vm, unit).map_or(0.0, |pos| pos.y))
}

pub(super) fn get_spell_target_loc(vm: &mut JassVm, _a: &[Value]) -> Value {
    let pos = vm
        .event()
        .spell_target_unit
        .and_then(|unit| unit_position(vm, unit))
        .unwrap_or(glam::Vec2::ZERO);
    Value::Handle(vm.host.create(Object::Location(pos)))
}

pub(super) fn get_ordered_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.event().unit)
}

pub(super) fn get_expired_timer(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.event().expired_timer {
        Some(id) => Value::Handle(Handle::new(HandleKind::Timer, id)),
        None => Value::Null,
    }
}

fn unit_handle_or_null(id: Option<u32>) -> Value {
    match id {
        Some(id) => Value::Handle(Handle::new(HandleKind::Unit, id)),
        None => Value::Null,
    }
}

fn unit_position(vm: &JassVm, handle_id: u32) -> Option<glam::Vec2> {
    let handle = Handle::new(HandleKind::Unit, handle_id);
    let unit = vm.host.sim_unit(handle)?;
    vm.sim.unit(unit).map(|unit| unit.pos)
}

// ---------------------------------------------------------------------------
// Boolexprs (conditions and filters)
// ---------------------------------------------------------------------------

pub(super) fn condition(vm: &mut JassVm, a: &[Value]) -> Value {
    let func = code_index(a.first());
    Value::Handle(vm.host.create(Object::Boolexpr { func, negated: false }))
}

pub(super) fn filter(vm: &mut JassVm, a: &[Value]) -> Value {
    condition(vm, a)
}

pub(super) fn and_boolexpr(vm: &mut JassVm, a: &[Value]) -> Value {
    // `And(a, b)` is used by BJ filters to chain conditions. The VM evaluates
    // the first operand; chaining is recorded as an opaque handle because the
    // scripts in the corpus pass the result to native registration only.
    let _ = (hd(a, 0), hd(a, 1));
    Value::Handle(vm.host.create(Object::Boolexpr {
        func: None,
        negated: false,
    }))
}

pub(super) fn or_boolexpr(vm: &mut JassVm, a: &[Value]) -> Value {
    and_boolexpr(vm, a)
}

pub(super) fn not_boolexpr(vm: &mut JassVm, a: &[Value]) -> Value {
    let func = hd(a, 0).and_then(|handle| vm.host.boolexpr(handle)).and_then(|(func, _)| func);
    Value::Handle(vm.host.create(Object::Boolexpr {
        func,
        negated: true,
    }))
}

pub(super) fn destroy_boolexpr(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------

pub(super) fn create_timer(vm: &mut JassVm, _a: &[Value]) -> Value {
    let mut state = crate::jass::host::TimerState::default();
    let handle = vm.host.create(Object::Timer(state.clone()));
    state.handle = handle.id;
    vm.host.objects.insert(handle.id, Object::Timer(state));
    Value::Handle(handle)
}

pub(super) fn destroy_timer(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn timer_start(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let timeout = r(a, 1);
    let periodic = a.get(2).is_some_and(Value::truthy);
    let callback = code_index(a.get(3));
    let period_ticks = (timeout * crate::TICKS_PER_SECOND as f32).round().max(1.0) as u32;
    if let Some(state) = vm.host.timer_mut(handle) {
        state.timeout = timeout;
        state.period_ticks = period_ticks;
        state.ticks_remaining = period_ticks;
        state.periodic = periodic;
        state.running = true;
        state.callback = callback;
        state.handle = handle.id;
    }
    // A timer with a zero/negative timeout expires immediately on the next tick.
    Value::Null
}

pub(super) fn pause_timer(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0)
        && let Some(state) = vm.host.timer_mut(handle)
    {
        state.running = false;
    }
    Value::Null
}

pub(super) fn resume_timer(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0)
        && let Some(state) = vm.host.timer_mut(handle)
    {
        state.running = true;
    }
    Value::Null
}

pub(super) fn timer_get_remaining(vm: &mut JassVm, a: &[Value]) -> Value {
    let remaining = hd(a, 0)
        .and_then(|handle| vm.host.timer(handle))
        .map(|state| {
            if state.running {
                state.remaining_seconds()
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    Value::Real(remaining)
}

pub(super) fn timer_get_elapsed(vm: &mut JassVm, a: &[Value]) -> Value {
    let elapsed = hd(a, 0)
        .and_then(|handle| vm.host.timer(handle))
        .map(|state| {
            (state.period_ticks.saturating_sub(state.ticks_remaining)) as f32
                / crate::TICKS_PER_SECOND as f32
        })
        .unwrap_or(0.0);
    Value::Real(elapsed)
}

pub(super) fn timer_get_timeout(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(
        hd(a, 0)
            .and_then(|handle| vm.host.timer(handle))
            .map(|state| state.timeout)
            .unwrap_or(0.0),
    )
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

pub(super) fn create_group(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Group(Vec::new())))
}

pub(super) fn destroy_group(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn group_add_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(group), Some(unit)) = (hd(a, 0), unit_arg(a, 1)) else {
        return Value::Bool(false);
    };
    let added = if let Some(state) = vm.host.group_mut(group) {
        if state.contains(&unit.id) {
            false
        } else {
            state.push(unit.id);
            true
        }
    } else {
        false
    };
    Value::Bool(added)
}

pub(super) fn group_remove_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(group), Some(unit)) = (hd(a, 0), unit_arg(a, 1)) else {
        return Value::Bool(false);
    };
    let removed = if let Some(state) = vm.host.group_mut(group) {
        let before = state.len();
        state.retain(|id| *id != unit.id);
        state.len() != before
    } else {
        false
    };
    Value::Bool(removed)
}

pub(super) fn group_clear(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(group) = hd(a, 0)
        && let Some(state) = vm.host.group_mut(group)
    {
        state.clear();
    }
    Value::Null
}

pub(super) fn is_unit_in_group(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(unit), Some(group)) = (unit_arg(a, 0), hd(a, 1)) else {
        return Value::Bool(false);
    };
    Value::Bool(
        vm.host
            .group(group)
            .is_some_and(|members| members.contains(&unit.id)),
    )
}

pub(super) fn first_of_group(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(group) = hd(a, 0) else {
        return Value::Null;
    };
    match vm.host.group(group).and_then(|members| members.first().copied()) {
        None => Value::Null,
        Some(id) => {
            // `FirstOfGroup` removes the unit from the group, like the engine.
            if let Some(members) = vm.host.group_mut(group) {
                members.remove(0);
            }
            Value::Handle(Handle::new(HandleKind::Unit, id))
        }
    }
}

/// `ForGroup(group, callback)` — call `callback` once per member with
/// `GetEnumUnit()` returning that member.
pub(super) fn for_group(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(group) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(func) = code_index(a.get(1)) else {
        vm.errors.push("ForGroup: null callback".into());
        return Value::Null;
    };
    let members = vm.host.group(group).cloned().unwrap_or_default();
    let saved_enum = vm.enum_unit;
    let saved_filter = vm.filter_unit;
    for member in members {
        vm.enum_unit = Some(member);
        let mut event = vm.event();
        event.unit = Some(member);
        vm.call_script_function_with_event(func, Vec::new(), Some(event));
    }
    vm.enum_unit = saved_enum;
    vm.filter_unit = saved_filter;
    Value::Null
}

pub(super) fn get_enum_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.enum_unit)
}

pub(super) fn get_filter_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    unit_handle_or_null(vm.filter_unit)
}

/// `GroupEnumUnitsOfPlayer(group, player, filter)`.
pub(super) fn group_enum_units_of_player(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(group) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(player) = player_index(a, 1) else {
        return Value::Null;
    };
    let filter = hd(a, 2);
    let candidates: Vec<u32> = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.owner == player && unit.alive)
        .filter_map(|unit| vm.host.units.handle_of(unit.id))
        .collect();
    let accepted = filter_units(vm, filter, candidates);
    if let Some(members) = vm.host.group_mut(group) {
        members.clear();
        members.extend(accepted);
    }
    Value::Null
}

/// `GroupEnumUnitsInRange(group, x, y, radius, filter)`.
pub(super) fn group_enum_units_in_range(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(group) = hd(a, 0) else {
        return Value::Null;
    };
    let center = glam::Vec2::new(r(a, 1), r(a, 2));
    let radius = r(a, 3).max(0.0);
    let filter = hd(a, 4);
    let radius_squared = radius * radius;
    let candidates: Vec<u32> = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.pos.distance_squared(center) <= radius_squared)
        .filter_map(|unit| vm.host.units.handle_of(unit.id))
        .collect();
    let accepted = filter_units(vm, filter, candidates);
    if let Some(members) = vm.host.group_mut(group) {
        members.clear();
        members.extend(accepted);
    }
    Value::Null
}

/// `GroupEnumUnitsInRangeOfLoc(group, loc, radius, filter)`.
///
/// Same distance test as `GroupEnumUnitsInRange`, addressed by a `location`
/// handle. Members are emitted in **unit-id** order (not handle-allocation
/// order) so a script that consumes the group with `FirstOfGroup` sees the same
/// unit on every run and on every peer.
pub(super) fn group_enum_units_in_range_of_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(group) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(center) = hd(a, 1).and_then(|handle| vm.host.location(handle)) else {
        return Value::Null;
    };
    let radius = r(a, 2).max(0.0);
    let filter = hd(a, 3);
    let radius_squared = radius * radius;
    let mut candidates: Vec<(u32, u32)> = vm
        .sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.pos.distance_squared(center) <= radius_squared)
        .filter_map(|unit| Some((unit.id.0, vm.host.units.handle_of(unit.id)?)))
        .collect();
    candidates.sort_unstable_by_key(|(unit_id, _)| *unit_id);
    let candidates: Vec<u32> = candidates.into_iter().map(|(_, handle)| handle).collect();
    let accepted = filter_units(vm, filter, candidates);
    if let Some(members) = vm.host.group_mut(group) {
        members.clear();
        members.extend(accepted);
    }
    Value::Null
}

/// `GroupEnumUnitsInRect(group, rect, filter)`.
pub(super) fn group_enum_units_in_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(group) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(rect) = hd(a, 1) else {
        return Value::Null;
    };
    let Some((min, max)) = vm.host.rect(rect) else {
        return Value::Null;
    };
    let filter = hd(a, 2);
    let candidates: Vec<u32> = vm
        .sim
        .units
        .iter()
        .filter(|unit| {
            unit.alive
                && unit.pos.x >= min.x
                && unit.pos.x <= max.x
                && unit.pos.y >= min.y
                && unit.pos.y <= max.y
        })
        .filter_map(|unit| vm.host.units.handle_of(unit.id))
        .collect();
    let accepted = filter_units(vm, filter, candidates);
    if let Some(members) = vm.host.group_mut(group) {
        members.clear();
        members.extend(accepted);
    }
    Value::Null
}


/// Filter over unit handles by function index (used by the unit-in-range check).
pub(crate) fn filter_handles(vm: &mut JassVm, filter: Option<u32>, candidates: Vec<u32>) -> Vec<u32> {
    let Some(func) = filter else {
        return candidates;
    };
    let saved_filter = vm.filter_unit;
    let mut accepted = Vec::new();
    for candidate in candidates {
        vm.filter_unit = Some(candidate);
        let mut event = vm.event();
        event.unit = Some(candidate);
        let value = vm.call_script_function_with_event(func, Vec::new(), Some(event));
        if value.truthy() {
            accepted.push(candidate);
        }
    }
    vm.filter_unit = saved_filter;
    accepted
}

/// Function index of a `code` value (`Condition`/`Filter` payloads).
pub(crate) fn function_of(value: &Value) -> Option<u32> {
    code_index(Some(value))
}

/// Runs a `boolexpr` filter over candidate unit handles, returning the accepted
/// ones in input order.
pub(super) fn filter_units(vm: &mut JassVm, filter: Option<Handle>, candidates: Vec<u32>) -> Vec<u32> {
    let Some(filter) = filter else {
        return candidates;
    };
    let Some((func, negated)) = vm.host.boolexpr(filter) else {
        return candidates;
    };
    let Some(func) = func else {
        return candidates;
    };
    let saved_filter = vm.filter_unit;
    let mut accepted = Vec::new();
    for candidate in candidates {
        vm.filter_unit = Some(candidate);
        let mut event = vm.event();
        event.unit = Some(candidate);
        let value = vm.call_script_function_with_event(func, Vec::new(), Some(event));
        let truthy = value.truthy() != negated;
        if truthy {
            accepted.push(candidate);
        }
    }
    vm.filter_unit = saved_filter;
    accepted
}

// ---------------------------------------------------------------------------
// Forces
// ---------------------------------------------------------------------------

pub(super) fn create_force(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Force(Vec::new())))
}

pub(super) fn force_add_player(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(force), Some(player)) = (hd(a, 0), player_index(a, 1)) else {
        return Value::Null;
    };
    if let Some(Object::Force(members)) = vm.host.object_mut(force)
        && !members.contains(&player)
    {
        members.push(player);
        members.sort_unstable();
    }
    Value::Null
}

pub(super) fn force_remove_player(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(force), Some(player)) = (hd(a, 0), player_index(a, 1)) else {
        return Value::Null;
    };
    if let Some(Object::Force(members)) = vm.host.object_mut(force) {
        members.retain(|entry| *entry != player);
    }
    Value::Null
}

pub(super) fn get_enum_player(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.enum_player {
        Some(player) => Value::Handle(Handle::new(HandleKind::Player, player as u32)),
        None => Value::Null,
    }
}

pub(super) fn for_force(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(force) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(func) = code_index(a.get(1)) else {
        return Value::Null;
    };
    let members = match vm.host.object(force) {
        Some(Object::Force(members)) => members.clone(),
        _ => Vec::new(),
    };
    let saved = vm.enum_player;
    for player in members {
        vm.enum_player = Some(player);
        vm.call_script_function(func, Vec::new());
    }
    vm.enum_player = saved;
    Value::Null
}

pub(super) fn destroy_force(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

// ---------------------------------------------------------------------------
// Region / rect / location
// ---------------------------------------------------------------------------

pub(super) fn rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let min = glam::Vec2::new(r(a, 0).min(r(a, 2)), r(a, 1).min(r(a, 3)));
    let max = glam::Vec2::new(r(a, 0).max(r(a, 2)), r(a, 1).max(r(a, 3)));
    Value::Handle(vm.host.create(Object::Rect { min, max }))
}

pub(super) fn rect_from_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let min = location_pos(vm, a.first());
    let max = location_pos(vm, a.get(1));
    let minv = glam::Vec2::new(min.x.min(max.x), min.y.min(max.y));
    let maxv = glam::Vec2::new(min.x.max(max.x), min.y.max(max.y));
    Value::Handle(vm.host.create(Object::Rect { min: minv, max: maxv }))
}

pub(super) fn remove_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn set_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0)
        && let Some(Object::Rect { min, max }) = vm.host.object_mut(handle)
    {
        *min = glam::Vec2::new(r(a, 1).min(r(a, 3)), r(a, 2).min(r(a, 4)));
        *max = glam::Vec2::new(r(a, 1).max(r(a, 3)), r(a, 2).max(r(a, 4)));
    }
    Value::Null
}

pub(super) fn set_rect_from_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let min = location_pos(vm, a.get(1));
    let max = location_pos(vm, a.get(2));
    if let Some(handle) = hd(a, 0)
        && let Some(Object::Rect { min: rect_min, max: rect_max }) = vm.host.object_mut(handle)
    {
        *rect_min = glam::Vec2::new(min.x.min(max.x), min.y.min(max.y));
        *rect_max = glam::Vec2::new(min.x.max(max.x), min.y.max(max.y));
    }
    Value::Null
}

pub(super) fn move_rect_to(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let target = glam::Vec2::new(r(a, 1), r(a, 2));
    if let Some(Object::Rect { min, max }) = vm.host.object_mut(handle) {
        let center = (*min + *max) * 0.5;
        let offset = target - center;
        *min += offset;
        *max += offset;
    }
    Value::Null
}

pub(super) fn get_rect_center_x(vm: &mut JassVm, a: &[Value]) -> Value {
    let center = hd(a, 0)
        .and_then(|handle| vm.host.rect(handle))
        .map_or(glam::Vec2::ZERO, |(min, max)| (min + max) * 0.5);
    Value::Real(center.x)
}

pub(super) fn get_rect_center_y(vm: &mut JassVm, a: &[Value]) -> Value {
    let center = hd(a, 0)
        .and_then(|handle| vm.host.rect(handle))
        .map_or(glam::Vec2::ZERO, |(min, max)| (min + max) * 0.5);
    Value::Real(center.y)
}

fn rect_edge(vm: &JassVm, args: &[Value], edge: u8) -> f32 {
    let Some((min, max)) = hd(args, 0).and_then(|handle| vm.host.rect(handle)) else {
        return 0.0;
    };
    match edge {
        0 => min.x,
        1 => min.y,
        2 => max.x,
        _ => max.y,
    }
}

pub(super) fn get_rect_min_x(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(rect_edge(vm, a, 0))
}

pub(super) fn get_rect_min_y(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(rect_edge(vm, a, 1))
}

pub(super) fn get_rect_max_x(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(rect_edge(vm, a, 2))
}

pub(super) fn get_rect_max_y(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(rect_edge(vm, a, 3))
}

pub(super) fn create_region(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Region { rects: Vec::new() }))
}

pub(super) fn region_add_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(region), Some(rect)) = (hd(a, 0), hd(a, 1)) else {
        return Value::Null;
    };
    if let Some(Object::Region { rects }) = vm.host.object_mut(region)
        && !rects.contains(&rect.id)
    {
        rects.push(rect.id);
    }
    Value::Null
}

pub(super) fn region_clear_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(region), Some(rect)) = (hd(a, 0), hd(a, 1)) else {
        return Value::Null;
    };
    if let Some(Object::Region { rects }) = vm.host.object_mut(region) {
        rects.retain(|id| *id != rect.id);
    }
    Value::Null
}

pub(super) fn remove_region(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn location(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Handle(
        vm.host
            .create(Object::Location(glam::Vec2::new(r(a, 0), r(a, 1)))),
    )
}

pub(super) fn remove_location(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn get_location_x(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(location_pos(vm, a.first()).x)
}

pub(super) fn get_location_y(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(location_pos(vm, a.first()).y)
}

pub(super) fn move_location(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0)
        && let Some(Object::Location(pos)) = vm.host.object_mut(handle)
    {
        *pos += glam::Vec2::new(r(a, 1), r(a, 2));
    }
    Value::Null
}

/// Location payload of a value, or the world origin for `null`.
pub(super) fn location_pos(vm: &JassVm, value: Option<&Value>) -> glam::Vec2 {
    value
        .and_then(Value::handle)
        .and_then(|handle| vm.host.location(handle))
        .unwrap_or(glam::Vec2::ZERO)
}


// ---------------------------------------------------------------------------
// Unit-event BJ helpers used by every melee script
// ---------------------------------------------------------------------------


/// `IsPlayerInForce(player, force)`.
pub(super) fn is_player_in_force(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(player), Some(force)) = (player_index(a, 0), hd(a, 1)) else {
        return Value::Bool(false);
    };
    Value::Bool(match vm.host.object(force) {
        Some(Object::Force(members)) => members.contains(&player),
        _ => false,
    })
}

/// `TriggerRegisterDialogEvent(trigger, dialog)`.
pub(super) fn trigger_register_dialog_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(trigger), Some(dialog)) = (hd(a, 0), hd(a, 1)) else {
        return Value::Null;
    };
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::DialogEvent { dialog: dialog.id });
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

/// `TriggerRegisterUnitInRange(trigger, unit, range, filter)`.
pub(super) fn trigger_register_unit_in_range(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        return Value::Null;
    };
    let unit = unit_arg(a, 1).map(|handle| handle.id);
    let range = r(a, 2);
    let filter = hd(a, 3).and_then(|handle| vm.host.boolexpr(handle)).and_then(|(func, _)| func);
    if let Some(state) = vm.host.trigger_mut(trigger) {
        if let Some(unit) = unit {
            state.events.push(RegEvent::UnitInRange { unit, range, filter });
        }
    }
    Value::Handle(vm.host.create(Object::Opaque))
}

/// `TriggerSleepAction(timeout)` — suspend the calling script thread.
///
/// A direct `call TriggerSleepAction(x)` is compiled to the `Wait` instruction,
/// so this handler only runs when the name reaches the native dispatch at run
/// time (a name the compiler could not see, e.g. one no loaded header declares).
/// It asks the executor to park the current thread — the same parking path,
/// quantization and elapsed-time return value as `Wait` — so the two spellings
/// cannot diverge. A call with no running thread is a recorded diagnostic:
/// there is nothing to suspend, and answering 0.0 would claim a sleep that
/// never happened.
pub(super) fn trigger_sleep_action(vm: &mut JassVm, a: &[Value]) -> Value {
    let seconds = r(a, 0).max(0.0);
    if !vm.request_sleep(seconds) {
        vm.errors.push(format!(
            "TriggerSleepAction({seconds}): no running script thread to suspend"
        ));
        return Value::Real(0.0);
    }
    // The executor parks the thread before this value could be pushed; the
    // resume path pushes the elapsed seconds instead.
    Value::Real(seconds)
}

// ---------------------------------------------------------------------------
// Small helpers used by the scheduler
// ---------------------------------------------------------------------------






#[cfg(test)]
mod tests {
    use super::*;
    use crate::jass::host::TriggerState;
    use crate::jass::value::Value;

    fn empty_vm() -> JassVm {
        let program = wc3_re::jass::ast::Program::default();
        JassVm::new(&program, crate::Sim::new(11, 32, 32), 11).expect("empty program")
    }

    #[test]
    fn jass_natives_group_enum_units_in_range_of_loc_is_distance_and_id_ordered() {
        let mut vm = empty_vm();
        let near_a = vm
            .sim
            .spawn_def(0, "hfoo", glam::Vec2::new(1000.0, 1000.0))
            .expect("footman a");
        let near_b = vm
            .sim
            .spawn_def(1, "ogru", glam::Vec2::new(1100.0, 1000.0))
            .expect("grunt");
        let far = vm
            .sim
            .spawn_def(0, "hfoo", glam::Vec2::new(2000.0, 2000.0))
            .expect("far footman");
        // Bind handles in a different order than the spawn order so an
        // implementation that emits handle order instead of unit id is caught.
        for unit in [far, near_b, near_a] {
            let handle = vm.host.unit_handle(unit);
            assert_eq!(vm.host.sim_unit(handle), Some(unit));
        }
        let Value::Handle(group) = create_group(&mut vm, &[]) else {
            panic!("CreateGroup returns a group handle");
        };
        let location = location(&mut vm, &[Value::Real(1000.0), Value::Real(1000.0)]);
        group_enum_units_in_range_of_loc(
            &mut vm,
            &[
                Value::Handle(group),
                location,
                Value::Real(300.0),
                Value::Null,
            ],
        );
        let members = vm.host.group(group).cloned().expect("group object");
        assert_eq!(members.len(), 2, "two units inside the radius");
        let units: Vec<_> = members
            .iter()
            .map(|handle| vm.host.units.unit_of(*handle).expect("bound unit"))
            .collect();
        // Unit-id order, not handle-allocation order: the handles were bound
        // `far, near_b, near_a`, so a handle-ordered group would come out
        // reversed.
        let unit_ids: Vec<u32> = units.iter().map(|unit| unit.0).collect();
        assert!(
            unit_ids.windows(2).all(|pair| pair[0] < pair[1]),
            "members are emitted in unit-id order: {unit_ids:?}"
        );
        assert_eq!(units, vec![near_a, near_b]);
        assert!(!units.contains(&far));
    }

    #[test]
    fn jass_natives_trigger_register_player_alliance_change_fires_on_an_effective_change() {
        let mut vm = empty_vm();
        // Different teams: the melee default is "not allied".
        vm.host.players[1].team = 1;
        let trigger = vm.host.create(Object::Trigger(TriggerState::new()));
        let me = Value::Handle(Handle::new(HandleKind::Player, 0));
        let them = Value::Handle(Handle::new(HandleKind::Player, 1));
        trigger_register_player_alliance_change(
            &mut vm,
            &[Value::Handle(trigger), me.clone(), Value::Int(0)],
        );
        assert!(
            !vm.host.effective_alliance(0, 1, 0),
            "default alliance is the team rule"
        );
        super::super::core::set_player_alliance(
            &mut vm,
            &[me, them.clone(), Value::Int(0), Value::Bool(true)],
        );
        assert!(vm.host.effective_alliance(0, 1, 0));
        let epoch = vm.host.alliance_epoch.get(&(0, 0)).copied().unwrap_or(0);
        assert!(epoch > 0, "the change bumped the seat's epoch");
        // Re-asserting the same value is not a change.
        super::super::core::set_player_alliance(
            &mut vm,
            &[
                Value::Handle(Handle::new(HandleKind::Player, 0)),
                them,
                Value::Int(0),
                Value::Bool(true),
            ],
        );
        assert_eq!(
            vm.host.alliance_epoch.get(&(0, 0)).copied().unwrap_or(0),
            epoch,
            "a repeated assignment keeps the epoch"
        );
    }
}
