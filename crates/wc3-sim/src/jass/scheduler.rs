//! The scheduler: turns simulation ticks into script time.
//!
//! Every tick the driver calls [`JassVm::advance`]. That single entry point
//! raises the trigger events the simulation produced, counts down timers and
//! trigger periods, and resumes threads parked on `TriggerSleepAction`. Nothing
//! here sleeps or spins: time only moves when the simulation steps, which is
//! what keeps a 10-minute map test reproducible.
//!
//! Event constants are resolved from the loaded `Scripts\common.j` globals (the
//! same header the map script was compiled against), so `EVENT_UNIT_DEATH` is
//! whatever the loaded header says — 53 in the 1.32.9 script data, different in
//! 1.27 — instead of a number baked into this file. [`EventIds::DEFAULT`] holds
//! the 1.32.9 values for scripts run without a header.

use std::collections::BTreeSet;

use crate::jass::host::{Object, RegEvent};
use crate::jass::value::{Handle, HandleKind, Value};
use crate::jass::vm::{EventContext, JassVm, RunOutcome};
use crate::Event;

/// Numeric ids of the event constants the scheduler can raise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventIds {
    pub unit_death: i64,
    pub unit_damaged: i64,
    pub unit_trained: i64,
    pub unit_construct: i64,
    pub unit_researched: i64,
    pub unit_hero_level: i64,
    pub unit_spell: i64,
    pub unit_orders: i64,
    pub unit_change_owner: i64,
    pub unit_attacked: i64,
    pub player_unit_death: i64,
    pub player_unit_damaged: i64,
    pub player_unit_trained: i64,
    pub player_unit_construct: i64,
    pub player_unit_researched: i64,
    pub player_hero_level: i64,
    pub player_unit_spell: i64,
    pub player_unit_orders: i64,
    pub player_unit_change_owner: i64,
    pub player_unit_attacked: i64,
    pub player_chat: i64,
    /// `EVENT_PLAYER_STATE_LIMIT` (raised by `TriggerRegisterPlayerStateEvent`).
    pub player_state_limit: i64,
    /// `EVENT_PLAYER_ALLIANCE_CHANGED` (raised by
    /// `TriggerRegisterPlayerAllianceChange`).
    pub player_alliance_changed: i64,
    /// `EVENT_GAME_STATE_LIMIT` (raised by `TriggerRegisterGameStateEvent`
    /// crossings and watched by `TriggerRegisterGameEvent`).
    pub game_state_limit: i64,
}

impl Default for EventIds {
    /// Values from `TestData/Jass/1.32.9/common.j` (the header in this tree),
    /// used when a map ships no header of its own. `from_vm` overrides every field
    /// from the loaded header; `jass_natives_event_id_defaults_match_the_shipped_header`
    /// pins these numbers against that file so a typo cannot hide behind the
    /// override.
    fn default() -> Self {
        Self {
            unit_death: 53,
            unit_damaged: 52,
            unit_trained: 71,
            unit_construct: 65,
            unit_researched: 74,
            unit_hero_level: 78,
            // line 844: `EVENT_UNIT_SPELL_EFFECT = ConvertUnitEvent(291)`
            unit_spell: 291,
            unit_orders: 75,
            // line 840: `EVENT_UNIT_CHANGE_OWNER = ConvertUnitEvent(287)`
            unit_change_owner: 287,
            unit_attacked: 62,
            player_unit_death: 20,
            player_unit_damaged: 308,
            player_unit_trained: 34,
            player_unit_construct: 28,
            player_unit_researched: 37,
            player_hero_level: 41,
            // line 829: `EVENT_PLAYER_UNIT_SPELL_EFFECT = ConvertPlayerUnitEvent(274)`
            player_unit_spell: 274,
            player_unit_orders: 38,
            // line 825: `EVENT_PLAYER_UNIT_CHANGE_OWNER = ConvertPlayerUnitEvent(270)`
            player_unit_change_owner: 270,
            player_unit_attacked: 18,
            player_chat: 16,
            player_state_limit: 11,
            player_alliance_changed: 12,
            // line 642: `EVENT_GAME_STATE_LIMIT = ConvertGameEvent(3)`.
            game_state_limit: crate::jass::host::EVENT_GAME_STATE_LIMIT,
        }
    }
}

impl EventIds {
    /// Reads the constants the loaded header actually defines.
    pub fn from_vm(vm: &JassVm) -> Self {
        let mut ids = Self::default();
        let read = |name: &str, slot: &mut i64| {
            if let Some(index) = vm.code.global_by_name.get(&name.to_ascii_lowercase())
                && let Some(Value::Int(value)) = vm.globals.get(*index as usize)
            {
                *slot = *value as i64;
            }
        };
        read("EVENT_UNIT_DEATH", &mut ids.unit_death);
        read("EVENT_UNIT_DAMAGED", &mut ids.unit_damaged);
        read("EVENT_UNIT_TRAIN_FINISH", &mut ids.unit_trained);
        read("EVENT_UNIT_CONSTRUCT_FINISH", &mut ids.unit_construct);
        read("EVENT_UNIT_RESEARCH_FINISH", &mut ids.unit_researched);
        read("EVENT_UNIT_HERO_LEVEL", &mut ids.unit_hero_level);
        read("EVENT_UNIT_SPELL_EFFECT", &mut ids.unit_spell);
        read("EVENT_UNIT_ISSUED_ORDER", &mut ids.unit_orders);
        read("EVENT_UNIT_CHANGE_OWNER", &mut ids.unit_change_owner);
        read("EVENT_UNIT_ATTACKED", &mut ids.unit_attacked);
        read("EVENT_PLAYER_UNIT_DEATH", &mut ids.player_unit_death);
        read("EVENT_PLAYER_UNIT_DAMAGED", &mut ids.player_unit_damaged);
        read("EVENT_PLAYER_UNIT_TRAIN_FINISH", &mut ids.player_unit_trained);
        read(
            "EVENT_PLAYER_UNIT_CONSTRUCT_FINISH",
            &mut ids.player_unit_construct,
        );
        read(
            "EVENT_PLAYER_UNIT_RESEARCH_FINISH",
            &mut ids.player_unit_researched,
        );
        read("EVENT_PLAYER_HERO_LEVEL", &mut ids.player_hero_level);
        read("EVENT_PLAYER_UNIT_SPELL_EFFECT", &mut ids.player_unit_spell);
        read("EVENT_PLAYER_UNIT_ISSUED_ORDER", &mut ids.player_unit_orders);
        read(
            "EVENT_PLAYER_UNIT_CHANGE_OWNER",
            &mut ids.player_unit_change_owner,
        );
        read("EVENT_PLAYER_UNIT_ATTACKED", &mut ids.player_unit_attacked);
        read("EVENT_PLAYER_CHAT", &mut ids.player_chat);
        read("EVENT_PLAYER_STATE_LIMIT", &mut ids.player_state_limit);
        read(
            "EVENT_PLAYER_ALLIANCE_CHANGED",
            &mut ids.player_alliance_changed,
        );
        read("EVENT_GAME_STATE_LIMIT", &mut ids.game_state_limit);
        ids
    }

    /// True when `constant` is the unit-scoped or player-scoped form of the
    /// same event family.
    fn matches(&self, constant: i64, unit: i64, player: i64) -> bool {
        constant == unit || constant == player
    }
}

/// One trigger activation computed from a simulation event.
struct Activation {
    trigger: u32,
    actions: Vec<u32>,
    conditions: Vec<u32>,
    context: EventContext,
}

impl JassVm {
    /// Raises the trigger events the simulation has queued without advancing
    /// script time. The driver calls it before stepping so an event raised
    /// between ticks (by the host or a test) is not swallowed by `Sim::step`'s
    /// event-buffer reset.
    pub fn dispatch_pending_events(&mut self) {
        self.dispatch_sim_events();
        self.run_game_events();
    }

    /// Advances script time by one simulation tick: raises events, runs timers,
    /// resumes parked threads. `dt` is seconds.
    pub fn advance(&mut self, dt: f64) {
        self.time += dt;
        self.tick += 1;
        self.dispatch_sim_events();
        self.run_player_watch_events();
        self.run_game_events();
        self.run_timers(dt);
        self.run_trigger_periods(dt);
        self.resume_threads();
    }

    /// Raises the two player-scoped registrations that are sampled once per
    /// tick rather than driven by a simulation event:
    /// `TriggerRegisterPlayerStateEvent` (fires on the false → true crossing of
    /// its `limitop` comparison) and `TriggerRegisterPlayerAllianceChange`
    /// (fires when the `(player, ALLIANCE_*)` epoch moved since the last pass).
    pub(crate) fn run_player_watch_events(&mut self) {
        // Only triggers that registered a per-tick player watch are cloned; a
        // map with none (the common case) pays one scan of the trigger table
        // and no allocation at all.
        let watched: Vec<(u32, crate::jass::host::TriggerState)> = self
            .host
            .objects
            .iter()
            .filter_map(|(id, object)| match object {
                Object::Trigger(state)
                    if state.enabled
                        && state.events.iter().any(|event| {
                            matches!(
                                event,
                                RegEvent::PlayerStateEvent { .. }
                                    | RegEvent::PlayerAllianceChange { .. }
                            )
                        }) =>
                {
                    Some((*id, state.clone()))
                }
                _ => None,
            })
            .collect();
        if watched.is_empty() {
            return;
        }
        let ids = self.event_ids;
        let mut activations: Vec<Activation> = Vec::new();
        for (trigger_id, state) in watched {
            for registered in &state.events {
                let (context, changed) = match registered {
                    RegEvent::PlayerStateEvent {
                        player,
                        state: which,
                        op,
                        value,
                    } => {
                        let key = (trigger_id, *player, *which, *op, value.to_bits());
                        let current =
                            super::natives::player_state_value(&self.sim, &self.host, *player, *which);
                        let Some(held) = super::natives::compare_limit(current, *op, *value) else {
                            continue;
                        };
                        let previous = self.host.state_watch.insert(key, held);
                        let fired = held && previous == Some(false);
                        (
                            EventContext {
                                event_id: ids.player_state_limit,
                                player: Some(*player),
                                ..EventContext::none()
                            },
                            fired,
                        )
                    }
                    RegEvent::PlayerAllianceChange { player, alliance } => {
                        let epoch = self
                            .host
                            .alliance_epoch
                            .get(&(*player, *alliance))
                            .copied()
                            .unwrap_or(0);
                        let key = (trigger_id, *player, *alliance);
                        let previous = self.host.alliance_watch.insert(key, epoch).unwrap_or(epoch);
                        (
                            EventContext {
                                event_id: ids.player_alliance_changed,
                                player: Some(*player),
                                ..EventContext::none()
                            },
                            epoch != previous,
                        )
                    }
                    _ => continue,
                };
                if changed {
                    activations.push(Activation {
                        trigger: trigger_id,
                        actions: state.actions.clone(),
                        conditions: state.conditions.clone(),
                        context,
                    });
                }
            }
        }
        for activation in activations {
            let handle = Handle::new(HandleKind::Trigger, activation.trigger);
            self.run_trigger_actions(
                handle,
                &activation.actions,
                activation.conditions,
                activation.context,
            );
        }
    }

    /// Raises the game-scoped registrations that are sampled once per tick or
    /// queued by the host:
    ///
    /// * `TriggerRegisterGameStateEvent` — the game state value (the same number
    ///   `GetFloatGameState`/`GetIntegerGameState` read) is compared with the
    ///   registration's `limitop`; the trigger fires on the false → true
    ///   crossing, the rule `TriggerRegisterPlayerStateEvent` uses. A crossing
    ///   also raises `EVENT_GAME_STATE_LIMIT` for every
    ///   `TriggerRegisterGameEvent` watch on that id, which is what the constant
    ///   means in the shipped header.
    /// * `TriggerRegisterGameEvent` — every event queued through
    ///   [`crate::jass::host::Host::raise_game_event`] is delivered once.
    pub(crate) fn run_game_events(&mut self) {
        let pending: Vec<i64> = std::mem::take(&mut self.host.pending_game_events);
        let watched: Vec<(u32, crate::jass::host::TriggerState)> = self
            .host
            .objects
            .iter()
            .filter_map(|(id, object)| match object {
                Object::Trigger(state)
                    if state.enabled
                        && state.events.iter().any(|event| {
                            matches!(
                                event,
                                RegEvent::GameStateEvent { .. } | RegEvent::GameEvent { .. }
                            )
                        }) =>
                {
                    Some((*id, state.clone()))
                }
                _ => None,
            })
            .collect();
        if watched.is_empty() && pending.is_empty() {
            return;
        }
        let ids = self.event_ids;
        let mut activations: Vec<Activation> = Vec::new();
        for (trigger_id, state) in &watched {
            for registered in &state.events {
                match registered {
                    RegEvent::GameStateEvent {
                        state: which,
                        op,
                        value,
                    } => {
                        let current = super::natives::game_state_value(&self.host, *which);
                        let Some(held) = super::natives::compare_limit(current, *op, *value)
                        else {
                            continue;
                        };
                        let key = (*trigger_id, *which, *op, value.to_bits());
                        let previous = self.host.game_state_watch.insert(key, held);
                        if held && previous == Some(false) {
                            activations.push(Activation {
                                trigger: *trigger_id,
                                actions: state.actions.clone(),
                                conditions: state.conditions.clone(),
                                context: EventContext {
                                    event_id: ids.game_state_limit,
                                    game_state: Some(*which),
                                    ..EventContext::none()
                                },
                            });
                            // `EVENT_GAME_STATE_LIMIT` is a game event: deliver it
                            // to the `TriggerRegisterGameEvent` watches too.
                            for (other_id, other) in &watched {
                                if other.enabled
                                    && other.events.iter().any(|registered| {
                                        matches!(
                                            registered,
                                            RegEvent::GameEvent { event }
                                                if *event == ids.game_state_limit
                                        )
                                    })
                                {
                                    activations.push(Activation {
                                        trigger: *other_id,
                                        actions: other.actions.clone(),
                                        conditions: other.conditions.clone(),
                                        context: EventContext {
                                            event_id: ids.game_state_limit,
                                            game_state: Some(*which),
                                            ..EventContext::none()
                                        },
                                    });
                                }
                            }
                        }
                    }
                    RegEvent::GameEvent { event } => {
                        if pending.contains(event) {
                            activations.push(Activation {
                                trigger: *trigger_id,
                                actions: state.actions.clone(),
                                conditions: state.conditions.clone(),
                                context: EventContext {
                                    event_id: *event,
                                    ..EventContext::none()
                                },
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        for activation in activations {
            let handle = Handle::new(HandleKind::Trigger, activation.trigger);
            self.run_trigger_actions(
                handle,
                &activation.actions,
                activation.conditions,
                activation.context,
            );
        }
    }

    /// Raises the trigger events the simulation produced on this tick.
    pub(crate) fn dispatch_sim_events(&mut self) {
        let events: Vec<Event> = self.sim.events.clone();
        if events.is_empty() {
            return;
        }
        let ids = self.event_ids;
        let mut activations: Vec<Activation> = Vec::new();
        for event in &events {
            match event {
                Event::UnitDied(unit) => {
                    let Some(handle) = self.host.units.handle_of(*unit) else {
                        continue;
                    };
                    let owner = self.sim.unit(*unit).map(|unit| unit.owner);
                    let context = EventContext {
                        dying_unit: Some(handle),
                        unit: Some(handle),
                        event_id: ids.unit_death,
                        player: owner,
                        ..EventContext::none()
                    };
                    self.collect_activations(
                        Some(handle),
                        owner,
                        &[ids.unit_death, ids.player_unit_death],
                        context,
                        &mut activations,
                    );
                }
                Event::Damage { from, to, amount } => {
                    let (Some(from_handle), Some(to_handle)) =
                        (self.host.units.handle_of(*from), self.host.units.handle_of(*to))
                    else {
                        continue;
                    };
                    let owner = self.sim.unit(*to).map(|unit| unit.owner);
                    let context = EventContext {
                        attacker: Some(from_handle),
                        damaged: Some(to_handle),
                        unit: Some(to_handle),
                        damage: *amount as f32,
                        event_id: ids.unit_damaged,
                        player: owner,
                        ..EventContext::none()
                    };
                    self.collect_activations(
                        Some(to_handle),
                        owner,
                        &[ids.unit_damaged, ids.player_unit_damaged],
                        context,
                        &mut activations,
                    );
                }
                Event::UnitTrained(unit) => {
                    let Some(handle) = self.host.units.handle_of(*unit) else {
                        continue;
                    };
                    let (owner, rawcode) = self
                        .sim
                        .unit(*unit)
                        .map(|unit| {
                            (
                                unit.owner,
                                super::natives::unit_rawcode(unit.def().id),
                            )
                        })
                        .unwrap_or((0, 0));
                    let context = EventContext {
                        trained_unit: Some(handle),
                        trained_unit_type: rawcode as i64,
                        unit: Some(handle),
                        event_id: ids.unit_trained,
                        player: Some(owner),
                        ..EventContext::none()
                    };
                    self.collect_activations(
                        Some(handle),
                        Some(owner),
                        &[ids.unit_trained, ids.player_unit_trained],
                        context,
                        &mut activations,
                    );
                }
                Event::BuildingComplete(unit) => {
                    let Some(handle) = self.host.units.handle_of(*unit) else {
                        continue;
                    };
                    let owner = self.sim.unit(*unit).map(|unit| unit.owner);
                    let context = EventContext {
                        unit: Some(handle),
                        event_id: ids.unit_construct,
                        player: owner,
                        ..EventContext::none()
                    };
                    self.collect_activations(
                        Some(handle),
                        owner,
                        &[ids.unit_construct, ids.player_unit_construct],
                        context,
                        &mut activations,
                    );
                }
                Event::UpgradeResearched { owner, .. } => {
                    let context = EventContext {
                        event_id: ids.unit_researched,
                        player: Some(*owner),
                        ..EventContext::none()
                    };
                    self.collect_activations(
                        None,
                        Some(*owner),
                        &[ids.unit_researched, ids.player_unit_researched],
                        context,
                        &mut activations,
                    );
                }
                Event::DestructableDestroyed { id } => {
                    let Some(handle) = self
                        .host
                        .destructable_binding
                        .iter()
                        .find(|(_, bound)| *bound == id)
                        .map(|(handle, _)| *handle)
                    else {
                        continue;
                    };
                    for (trigger_id, state) in self.triggers_snapshot() {
                        let registered = state.events.iter().any(|registered| match registered {
                            RegEvent::DestructableDeath { destructable } => {
                                destructable.is_none() || *destructable == Some(handle)
                            }
                            _ => false,
                        });
                        if registered {
                            activations.push(Activation {
                                trigger: trigger_id,
                                actions: state.actions.clone(),
                                conditions: state.conditions.clone(),
                                context: EventContext {
                                    dying_destructable: Some(handle),
                                    unit: None,
                                    event_id: ids.unit_death,
                                    ..EventContext::none()
                                },
                            });
                        }
                    }
                }
                Event::SpellCast { caster, .. } | Event::DataAbilityCast { caster, .. } => {
                    let Some(handle) = self.host.units.handle_of(*caster) else {
                        continue;
                    };
                    let owner = self.sim.unit(*caster).map(|unit| unit.owner);
                    let spell_id = match event {
                        Event::SpellCast { spell, .. } => super::natives::spell_rawcode(*spell),
                        Event::DataAbilityCast { ability, .. } => {
                            super::natives::unit_rawcode(ability)
                        }
                        _ => 0,
                    };
                    let target = match event {
                        Event::DataAbilityCast { target, .. } => *target,
                        _ => None,
                    };
                    let target_handle = target.and_then(|unit| self.host.units.handle_of(unit));
                    let context = EventContext {
                        unit: Some(handle),
                        spell_id: spell_id as i64,
                        spell_target_unit: target_handle,
                        event_id: ids.unit_spell,
                        player: owner,
                        ..EventContext::none()
                    };
                    self.collect_activations(
                        Some(handle),
                        owner,
                        &[ids.unit_spell, ids.player_unit_spell],
                        context,
                        &mut activations,
                    );
                }
                _ => {}
            }
        }
        for activation in activations {
            let handle = Handle::new(HandleKind::Trigger, activation.trigger);
            self.run_trigger_actions(
                handle,
                &activation.actions,
                activation.conditions,
                activation.context,
            );
        }
    }

    /// Collects the triggers registered for `constants`, scoped to a unit and/or
    /// a player.
    fn collect_activations(
        &mut self,
        unit: Option<u32>,
        owner: Option<u8>,
        constants: &[i64],
        context: EventContext,
        out: &mut Vec<Activation>,
    ) {
        for (trigger_id, state) in self.triggers_snapshot() {
            if !state.enabled {
                continue;
            }
            let mut matched = false;
            for registered in &state.events {
                match registered {
                    RegEvent::UnitEvent {
                        unit: registered_unit,
                        event,
                    } => {
                        if unit == Some(*registered_unit) && constants.contains(event) {
                            matched = true;
                            break;
                        }
                    }
                    RegEvent::AnyUnitEvent { event, player } => {
                        if !constants.contains(event) {
                            continue;
                        }
                        match player {
                            Some(player) => {
                                if owner == Some(*player) {
                                    matched = true;
                                    break;
                                }
                            }
                            None => {
                                matched = true;
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if matched {
                out.push(Activation {
                    trigger: trigger_id,
                    actions: state.actions.clone(),
                    conditions: state.conditions.clone(),
                    context: context.clone(),
                });
            }
        }
    }

    /// Deterministic snapshot of every trigger with its actions and conditions.
    fn triggers_snapshot(&self) -> Vec<(u32, crate::jass::host::TriggerState)> {
        self.host
            .objects
            .iter()
            .filter_map(|(id, object)| match object {
                Object::Trigger(state) => Some((*id, state.clone())),
                _ => None,
            })
            .collect()
    }

    /// Runs a trigger: conditions first (when `check_conditions`), then actions.
    pub fn run_trigger_actions(
        &mut self,
        trigger: Handle,
        actions: &[u32],
        conditions: Vec<u32>,
        context: EventContext,
    ) {
        if !self.evaluate_conditions(&conditions, context.clone()) {
            return;
        }
        if let Some(state) = self.host.trigger_mut(trigger) {
            if state.running {
                // The engine refuses recursive execution of the same trigger.
                return;
            }
            state.running = true;
        }
        let mut action_context = context;
        action_context.trigger = Some(trigger.id);
        for action in actions {
            self.call_script_function_with_event(*action, Vec::new(), Some(action_context.clone()));
        }
        if let Some(state) = self.host.trigger_mut(trigger) {
            state.running = false;
        }
    }

    /// Evaluates condition functions; all must return true.
    pub fn evaluate_conditions(&mut self, conditions: &[u32], context: EventContext) -> bool {
        for condition in conditions {
            let value =
                self.call_script_function_with_event(*condition, Vec::new(), Some(context.clone()));
            if !value.truthy() {
                return false;
            }
        }
        true
    }

    /// Advances timers and fires their callbacks.
    fn run_timers(&mut self, _dt: f64) {
        let handles: Vec<u32> = self
            .host
            .objects
            .iter()
            .filter_map(|(id, object)| match object {
                Object::Timer(_) => Some(*id),
                _ => None,
            })
            .collect();
        let mut fired: Vec<(u32, Option<u32>, bool, u32)> = Vec::new();
        for handle_id in handles {
            let handle = Handle::new(HandleKind::Timer, handle_id);
            let Some(state) = self.host.timer_mut(handle) else {
                continue;
            };
            if !state.running {
                continue;
            }
            state.ticks_remaining = state.ticks_remaining.saturating_sub(1);
            if state.ticks_remaining > 0 {
                continue;
            }
            let callback = state.callback;
            let periodic = state.periodic;
            let period_ticks = state.period_ticks;
            if periodic {
                // Keep the phase on the tick grid, so a 0.1 s timer fires every
                // three ticks instead of drifting.
                state.ticks_remaining = period_ticks.max(1);
            } else {
                state.running = false;
                state.ticks_remaining = 0;
            }
            fired.push((handle_id, callback, periodic, period_ticks));
        }
        for (handle_id, callback, periodic, period_ticks) in fired {
            if let Some(callback) = callback {
                let context = EventContext {
                    expired_timer: Some(handle_id),
                    ..EventContext::none()
                };
                self.call_script_function_with_event(callback, Vec::new(), Some(context));
            }
            // `TriggerRegisterTimerExpireEvent` triggers on this timer.
            let activations: Vec<Activation> = self
                .triggers_snapshot()
                .into_iter()
                .filter(|(_, state)| {
                    state.enabled
                        && state
                            .events
                            .iter()
                            .any(|event| matches!(event, RegEvent::TimerExpire { timer } if *timer == handle_id))
                })
                .map(|(trigger, state)| Activation {
                    trigger,
                    actions: state.actions,
                    conditions: state.conditions,
                    context: EventContext {
                        expired_timer: Some(handle_id),
                        event_id: 4,
                        ..EventContext::none()
                    },
                })
                .collect();
            for activation in activations {
                self.run_trigger_actions(
                    Handle::new(HandleKind::Trigger, activation.trigger),
                    &activation.actions,
                    activation.conditions,
                    activation.context,
                );
            }
            if !periodic {
                let _ = period_ticks;
            }
        }
    }

    /// Counts down `TriggerRegisterTimerEvent` periods.
    fn run_trigger_periods(&mut self, _dt: f64) {
        let ids: Vec<u32> = self
            .host
            .objects
            .iter()
            .filter_map(|(id, object)| match object {
                Object::Trigger(_) => Some(*id),
                _ => None,
            })
            .collect();
        let mut due: Vec<(u32, EventContext)> = Vec::new();
        for handle_id in ids {
            let Some(state) = self.host.trigger_mut(Handle::new(HandleKind::Trigger, handle_id)) else {
                continue;
            };
            if !state.enabled {
                continue;
            }
            for event in &mut state.events {
                if let RegEvent::TimerEvent {
                    period,
                    period_ticks,
                    periodic,
                    elapsed,
                } = event
                {
                    let _ = period;
                    *elapsed += 1;
                    if *elapsed >= *period_ticks {
                        if *periodic {
                            *elapsed = elapsed.saturating_sub(*period_ticks);
                        } else {
                            *elapsed = 0;
                        }
                        due.push((
                            handle_id,
                            EventContext {
                                trigger: Some(handle_id),
                                event_id: 4,
                                ..EventContext::none()
                            },
                        ));
                    }
                }
            }
        }
        for (handle_id, context) in due {
            let Some(state) = self
                .host
                .trigger(Handle::new(HandleKind::Trigger, handle_id))
                .cloned()
            else {
                continue;
            };
            self.run_trigger_actions(
                Handle::new(HandleKind::Trigger, handle_id),
                &state.actions,
                state.conditions,
                context,
            );
        }
    }

    /// Resumes threads parked on `TriggerSleepAction`.
    fn resume_threads(&mut self) {
        let mut index = 0;
        while index < self.threads.len() {
            let ready = self.threads[index]
                .wake_tick
                .is_some_and(|wake| self.tick >= wake);
            if !ready {
                index += 1;
                continue;
            }
            // `TriggerSleepAction` returns the seconds it actually slept.
            if let Some(started) = self.threads[index].wait_started.take() {
                let elapsed = (self.time - started).max(0.0) as f32;
                let base = self.threads[index].stack.len();
                let _ = base;
                self.threads[index].stack.push(Value::Real(elapsed));
            }
            self.threads[index].wake_at = None;
            self.threads[index].wake_tick = None;
            match self.drive_thread(index) {
                Ok(RunOutcome::Returned) | Ok(RunOutcome::Trapped) => {
                    self.threads.swap_remove(index);
                }
                Ok(RunOutcome::Suspended) => index += 1,
                Err(trap) => {
                    let name = self.threads[index].name.clone();
                    self.errors.push(format!("{name}: {trap}"));
                    self.threads.swap_remove(index);
                }
            }
        }
    }

    /// Delivers a player chat message to `TriggerRegisterPlayerChatEvent`
    /// triggers. Used by tests and by the console bridge.
    pub fn deliver_chat(&mut self, player: u8, message: &str) {
        let ids = self.event_ids;
        let activations: Vec<Activation> = self
            .triggers_snapshot()
            .into_iter()
            .filter(|(_, state)| {
                state.enabled
                    && state.events.iter().any(|event| match event {
                        RegEvent::Chat {
                            player: registered,
                            message: pattern,
                            exact,
                        } => {
                            *registered == player
                                && if *exact {
                                    message.eq_ignore_ascii_case(pattern)
                                } else {
                                    message.contains(pattern.as_str())
                                }
                        }
                        _ => false,
                    })
            })
            .map(|(trigger, state)| Activation {
                trigger,
                actions: state.actions,
                conditions: state.conditions,
                context: EventContext {
                    trigger: Some(trigger),
                    player: Some(player),
                    event_id: ids.player_chat,
                    ..EventContext::none()
                },
            })
            .collect();
        for activation in activations {
            self.run_trigger_actions(
                Handle::new(HandleKind::Trigger, activation.trigger),
                &activation.actions,
                activation.conditions,
                activation.context,
            );
        }
    }

    /// Region enter/leave tracking: `TriggerRegisterEnterRegion` really fires
    /// when a unit crosses into a registered region.
    pub(crate) fn track_regions(&mut self) {
        let region_ids: BTreeSet<u32> = self
            .host
            .objects
            .iter()
            .filter_map(|(id, object)| match object {
                Object::Region { .. } => Some(*id),
                _ => None,
            })
            .filter(|id| self.region_has_trigger(*id))
            .collect();
        if region_ids.is_empty() {
            return;
        }
        for region_id in region_ids {
            let rects: Vec<u32> = match self.host.object(Handle::new(HandleKind::Region, region_id)) {
                Some(Object::Region { rects }) => rects.clone(),
                _ => continue,
            };
            let bounds: Vec<(glam::Vec2, glam::Vec2)> = rects
                .into_iter()
                .filter_map(|rect| self.host.rect(Handle::new(HandleKind::Rect, rect)))
                .collect();
            let mut inside: BTreeSet<u32> = BTreeSet::new();
            for unit in self.sim.units.iter() {
                if !unit.alive {
                    continue;
                }
                let contained = bounds.iter().any(|(min, max)| {
                    unit.pos.x >= min.x
                        && unit.pos.x <= max.x
                        && unit.pos.y >= min.y
                        && unit.pos.y <= max.y
                });
                if contained
                    && let Some(handle) = self.host.units.handle_of(unit.id)
                {
                    inside.insert(handle);
                }
            }
            let previous = self.region_occupancy.get(&region_id).cloned().unwrap_or_default();
            let entered: Vec<u32> = inside.difference(&previous).copied().collect();
            let left: Vec<u32> = previous.difference(&inside).copied().collect();
            self.region_occupancy.insert(region_id, inside);
            for handle in entered {
                self.fire_region_event(region_id, handle, true);
            }
            for handle in left {
                self.fire_region_event(region_id, handle, false);
            }
        }
    }

    /// `TriggerRegisterUnitInRange`: fires when another unit enters the radius of
    /// the watched unit.
    pub(crate) fn track_unit_ranges(&mut self) {
        let mut watched: Vec<(u32, u32, f32, Option<u32>)> = Vec::new();
        for (trigger, state) in self.triggers_snapshot() {
            if !state.enabled {
                continue;
            }
            for event in &state.events {
                if let RegEvent::UnitInRange { unit, range, filter } = event {
                    watched.push((trigger, *unit, *range, *filter));
                }
            }
        }
        for (trigger, watched_unit, range, filter) in watched {
            let Some(center_handle) = self.host.sim_unit(Handle::new(HandleKind::Unit, watched_unit))
            else {
                continue;
            };
            let Some(center) = self.sim.unit(center_handle).map(|unit| unit.pos) else {
                continue;
            };
            let radius = range.max(0.0);
            let radius_squared = radius * radius;
            let candidates: Vec<u32> = self
                .sim
                .units
                .iter()
                .filter(|unit| unit.alive)
                .filter(|unit| unit.pos.distance_squared(center) <= radius_squared)
                .filter_map(|unit| self.host.units.handle_of(unit.id))
                .filter(|handle| *handle != watched_unit)
                .collect();
            let accepted = super::natives::trigger::filter_handles(self, filter, candidates);
            let key = (trigger, watched_unit);
            let previous = self
                .unit_range_occupancy
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let entered: Vec<u32> = accepted
                .iter()
                .copied()
                .filter(|handle| !previous.contains(handle))
                .collect();
            self.unit_range_occupancy
                .insert(key, accepted.iter().copied().collect());
            for unit in entered {
                let actions = self
                    .host
                    .trigger(Handle::new(HandleKind::Trigger, trigger))
                    .cloned();
                if let Some(state) = actions {
                    self.run_trigger_actions(
                        Handle::new(HandleKind::Trigger, trigger),
                        &state.actions,
                        state.conditions,
                        EventContext {
                            trigger: Some(trigger),
                            unit: Some(unit),
                            entering_unit: Some(unit),
                            ..EventContext::none()
                        },
                    );
                }
            }
        }
    }

    fn region_has_trigger(&self, region: u32) -> bool {
        self.host.objects.values().any(|object| match object {
            Object::Trigger(state) => state.events.iter().any(|event| match event {
                RegEvent::RegionEnter { region: id } | RegEvent::RegionLeave { region: id } => {
                    *id == region
                }
                _ => false,
            }),
            _ => false,
        })
    }

    fn fire_region_event(&mut self, region: u32, unit: u32, entering: bool) {
        let activations: Vec<Activation> = self
            .triggers_snapshot()
            .into_iter()
            .filter(|(_, state)| {
                state.enabled
                    && state.events.iter().any(|event| match event {
                        RegEvent::RegionEnter { region: id } => entering && *id == region,
                        RegEvent::RegionLeave { region: id } => !entering && *id == region,
                        _ => false,
                    })
            })
            .map(|(trigger, state)| Activation {
                trigger,
                actions: state.actions,
                conditions: state.conditions,
                context: EventContext {
                    trigger: Some(trigger),
                    unit: Some(unit),
                    entering_unit: entering.then_some(unit),
                    leaving_unit: (!entering).then_some(unit),
                    ..EventContext::none()
                },
            })
            .collect();
        for activation in activations {
            self.run_trigger_actions(
                Handle::new(HandleKind::Trigger, activation.trigger),
                &activation.actions,
                activation.conditions,
                activation.context,
            );
        }
    }

    /// True when `constant` is the `EVENT_UNIT_DEATH`-family id of this map.
    pub fn is_death_event(&self, constant: i64) -> bool {
        self.event_ids.matches(constant, self.event_ids.unit_death, self.event_ids.player_unit_death)
    }
}
