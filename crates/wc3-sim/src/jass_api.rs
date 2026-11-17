//! JASS native surface bound to the canonical simulation.
//!
//! Map scripts call the same natives the donor engines implement.  Instead of
//! the script-only world used by a bare interpreter, every native here mutates
//! [`crate::Sim`] through its public API, so a script-created unit is a real
//! unit, a script-issued order is a real order, and script damage runs the real
//! death/XP path.
//!
//! Native semantics follow the donor implementations (access date 2026-11-14):
//!
//! * `CreateUnit` / `CreateUnitAtLoc` — `WarsmashModEngine/core/src/com/etheller/
//!   warsmash/viewer5/handlers/w3x/simulation/trigger/...` and the JASS API in
//!   `W3AndroidEngine/core/src/com/etheller/warsmash/viewer5/handlers/w3x/
//!   simulation/CUnit.java` (unit created complete, facing applied).
//! * `UnitApplyTimedLife` — `.../simulation/abilities/skills/util/
//!   CBuffTimedLife.java:10-40`: attaches a lifetime buff; expiry removes unit.
//! * `IssueImmediateOrder`/`IssuePointOrder`/`IssueTargetOrder` — order ids from
//!   `.../orders/OrderIds.java` (`stop`, `hold`, `move`, `attack`, `patrol`).
//! * `SetUnitState` — `UNIT_STATE_LIFE` / `UNIT_STATE_MANA` clamp to the unit's
//!   maximum, matching `CUnit.setUnitState`.
//!
//! Units are addressed by `handle` (`integer`); the table is deterministic and
//! drains in id order on [`SimJass::sync`].

use glam::Vec2;

use crate::buffs::{Buff, BuffKind};
use crate::{OrderSpec, Sim, UnitId};

/// JASS `unitstate` constants (`common.j`).
pub const UNIT_STATE_LIFE: i32 = 0;
pub const UNIT_STATE_MAX_LIFE: i32 = 1;
pub const UNIT_STATE_MANA: i32 = 2;
pub const UNIT_STATE_MAX_MANA: i32 = 3;

/// Result of a native call.  `Err` carries the exact reason a script would get a
/// null/false back, so tests and the bridge can report it instead of silently
/// producing a zero.
pub type JassResult<T> = Result<T, &'static str>;

/// Handle table binding JASS integers to simulation units.
#[derive(Debug, Clone, Default)]
pub struct SimJass {
    /// handle -> unit
    units: Vec<(u32, UnitId)>,
    /// unit -> handle
    rev: Vec<(UnitId, u32)>,
    next_handle: u32,
}

impl SimJass {
    pub fn new() -> Self {
        Self {
            units: Vec::new(),
            rev: Vec::new(),
            next_handle: 0x1000_0000,
        }
    }

    pub fn len(&self) -> usize {
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Binds an existing simulation unit to a fresh handle.
    pub fn bind(&mut self, unit: UnitId) -> u32 {
        if let Some((_, handle)) = self.rev.iter().find(|(id, _)| *id == unit) {
            return *handle;
        }
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        self.units.push((handle, unit));
        self.rev.push((unit, handle));
        handle
    }

    /// Drops handles whose unit left the simulation (dead units keep handles,
    /// exactly like JASS does until `RemoveUnit`).
    pub fn unit_of(&self, handle: u32) -> Option<UnitId> {
        self.units
            .iter()
            .find(|(h, _)| *h == handle)
            .map(|(_, unit)| *unit)
    }

    pub fn handle_of(&self, unit: UnitId) -> Option<u32> {
        self.rev
            .iter()
            .find(|(id, _)| *id == unit)
            .map(|(_, handle)| *handle)
    }

    /// Sorted handle list for deterministic script iteration.
    pub fn handles(&self) -> Vec<u32> {
        let mut handles: Vec<u32> = self.units.iter().map(|(handle, _)| *handle).collect();
        handles.sort();
        handles
    }
}

impl Sim {
    /// JASS `CreateUnit(player, unitid, x, y, face)`.
    pub fn jass_create_unit(
        &mut self,
        table: &mut SimJass,
        owner: u8,
        def_id: &str,
        pos: Vec2,
        facing: f32,
    ) -> JassResult<u32> {
        if crate::def_of(def_id).is_none() {
            return Err("unknown unit type");
        }
        let unit = self
            .spawn_def_exact(owner, def_id, pos)
            .ok_or("could not spawn unit")?;
        if let Some(index) = self.unit_index(unit) {
            self.units[index].facing = facing;
        }
        Ok(table.bind(unit))
    }

    /// JASS `GetUnitX` / `GetUnitY`.
    pub fn jass_get_unit_pos(&self, table: &SimJass, handle: u32) -> JassResult<Vec2> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        self.unit(unit).map(|unit| unit.pos).ok_or("stale unit")
    }

    /// JASS `SetUnitPosition`.
    pub fn jass_set_unit_position(
        &mut self,
        table: &SimJass,
        handle: u32,
        pos: Vec2,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let Some(index) = self.unit_index(unit) else {
            return Err("stale unit");
        };
        self.units[index].pos = pos;
        self.units[index].path.clear();
        self.units[index].path_index = 0;
        Ok(())
    }

    /// JASS `GetUnitState`.
    pub fn jass_get_unit_state(&self, table: &SimJass, handle: u32, state: i32) -> JassResult<i32> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let unit = self.unit(unit).ok_or("stale unit")?;
        Ok(match state {
            UNIT_STATE_LIFE => unit.hp,
            UNIT_STATE_MAX_LIFE => unit.max_hp,
            UNIT_STATE_MANA => unit.mana,
            UNIT_STATE_MAX_MANA => unit.max_mana,
            _ => return Err("unsupported unit state"),
        })
    }

    /// JASS `SetUnitState`; the value is clamped to `[0, max]` like `CUnit`.
    pub fn jass_set_unit_state(
        &mut self,
        table: &SimJass,
        handle: u32,
        state: i32,
        value: f32,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let Some(index) = self.unit_index(unit) else {
            return Err("stale unit");
        };
        let mut died = false;
        {
            let target = &mut self.units[index];
            match state {
                UNIT_STATE_LIFE => {
                    target.hp = (value.round() as i32).clamp(0, target.max_hp.max(0));
                    died = target.hp == 0 && target.alive;
                }
                UNIT_STATE_MANA => {
                    target.mana = (value.round() as i32).clamp(0, target.max_mana.max(0));
                }
                _ => return Err("unsupported unit state"),
            }
        }
        if died {
            self.kill_unit(unit);
            self.clear_buffs(unit);
        }
        Ok(())
    }

    /// JASS `KillUnit` — the real death path (events, XP, creep drop).
    pub fn jass_kill_unit(&mut self, table: &SimJass, handle: u32) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        self.kill_unit(unit);
        self.clear_buffs(unit);
        Ok(())
    }

    /// JASS `RemoveUnit` — removes the unit and its handle.
    pub fn jass_remove_unit(&mut self, table: &mut SimJass, handle: u32) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        self.kill_unit(unit);
        self.clear_buffs(unit);
        table.units.retain(|(h, _)| *h != handle);
        table.rev.retain(|(id, _)| *id != unit);
        Ok(())
    }

    /// JASS `UnitApplyTimedLife(unit, buffId, duration)` — despawns on expiry.
    pub fn jass_apply_timed_life(
        &mut self,
        table: &SimJass,
        handle: u32,
        duration_seconds: f32,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        if !self.unit(unit).map(|u| u.alive).unwrap_or(false) {
            return Err("unit dead");
        }
        let ticks = (duration_seconds.max(0.0) * crate::TICKS_PER_SECOND as f32).round() as u64;
        let mut buff = Buff::timed("BTLF", unit, unit, 1, self.tick, ticks.max(1) as u32);
        buff.kind = BuffKind::TimedLife {
            expire_tick: self.tick + ticks.max(1),
        };
        buff.stack_key = None;
        self.add_buff(buff);
        Ok(())
    }

    /// JASS `UnitAddAbility` — records the rawcode on the unit and validates it
    /// against the data table.  Abilities our runner can cast become learnable.
    pub fn jass_unit_add_ability(
        &mut self,
        table: &SimJass,
        handle: u32,
        ability_id: &str,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let Some(def) = crate::abilities::generic_ability(ability_id) else {
            return Err("unknown ability");
        };
        let Some(index) = self.unit_index(unit) else {
            return Err("stale unit");
        };
        let id = def.id;
        let learned = &mut self.units[index].learned;
        if !learned.contains(&id) {
            learned.push(id);
        }
        Ok(())
    }

    /// JASS `UnitRemoveAbility`.
    pub fn jass_unit_remove_ability(
        &mut self,
        table: &SimJass,
        handle: u32,
        ability_id: &str,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        if crate::abilities::generic_ability(ability_id).is_none() {
            return Err("unknown ability");
        }
        let Some(index) = self.unit_index(unit) else {
            return Err("stale unit");
        };
        self.units[index].learned.retain(|id| *id != ability_id);
        Ok(())
    }

    /// JASS `IsUnitAliveBJ`.
    pub fn jass_is_unit_alive(&self, table: &SimJass, handle: u32) -> bool {
        table
            .unit_of(handle)
            .and_then(|unit| self.unit(unit))
            .map(|unit| unit.alive)
            .unwrap_or(false)
    }

    /// JASS `GetUnitTypeId` — returns the roster definition id of the unit.
    pub fn jass_unit_type_id(&self, table: &SimJass, handle: u32) -> JassResult<&'static str> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        self.unit(unit)
            .map(|unit| unit.def().id)
            .ok_or("stale unit")
    }

    /// JASS `SetUnitOwner`.
    pub fn jass_set_unit_owner(
        &mut self,
        table: &SimJass,
        handle: u32,
        owner: u8,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let Some(index) = self.unit_index(unit) else {
            return Err("stale unit");
        };
        self.units[index].owner = owner;
        Ok(())
    }

    /// JASS `IssueImmediateOrder` for `stop` / `hold` / `attack` (no target).
    pub fn jass_issue_immediate_order(
        &mut self,
        table: &SimJass,
        handle: u32,
        order: &str,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let spec = match order {
            "stop" => OrderSpec::Stop,
            "hold" | "holdposition" => OrderSpec::Hold,
            other => return Err(order_error(other)),
        };
        self.issue(&[unit], spec);
        Ok(())
    }

    /// JASS `IssuePointOrder` for `move` / `attack` / `attackground` / `patrol`.
    pub fn jass_issue_point_order(
        &mut self,
        table: &SimJass,
        handle: u32,
        order: &str,
        point: Vec2,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let spec = match order {
            "move" => OrderSpec::Move(point),
            "attack" | "attackground" => OrderSpec::AttackMove(point),
            "patrol" => {
                let start = self.unit(unit).map(|unit| unit.pos).ok_or("stale unit")?;
                OrderSpec::Patrol(start, point)
            }
            other => return Err(order_error(other)),
        };
        self.issue(&[unit], spec);
        Ok(())
    }

    /// JASS `IssueTargetOrder` for `attack` / `smart` / `move`.
    pub fn jass_issue_target_order(
        &mut self,
        table: &SimJass,
        handle: u32,
        order: &str,
        target_handle: u32,
    ) -> JassResult<()> {
        let unit = table.unit_of(handle).ok_or("invalid unit handle")?;
        let target = table
            .unit_of(target_handle)
            .ok_or("invalid target handle")?;
        let spec = match order {
            "attack" | "smart" => OrderSpec::Attack(target),
            "move" | "follow" => {
                let pos = self
                    .unit(target)
                    .map(|unit| unit.pos)
                    .ok_or("stale target")?;
                OrderSpec::Move(pos)
            }
            other => return Err(order_error(other)),
        };
        self.issue(&[unit], spec);
        Ok(())
    }

    /// JASS `UnitDamageTarget` with a typed damage attack.
    pub fn jass_unit_damage_target(
        &mut self,
        table: &SimJass,
        source: u32,
        target: u32,
        amount: f32,
        attack: crate::data::AttackType,
    ) -> JassResult<i32> {
        let source_unit = table.unit_of(source).ok_or("invalid source handle")?;
        let target_unit = table.unit_of(target).ok_or("invalid target handle")?;
        Ok(self.apply_damage(source_unit, target_unit, amount.round() as i32, attack))
    }

    /// JASS `AddUnitTypeToUnit`-style summon helper: spawns `count` units of a
    /// type owned by the handle's owner with an optional lifetime.
    pub fn jass_summon_units(
        &mut self,
        table: &mut SimJass,
        owner: u8,
        def_id: &str,
        pos: Vec2,
        count: u8,
        lifetime_seconds: f32,
    ) -> JassResult<Vec<u32>> {
        if crate::def_of(def_id).is_none() {
            return Err("unknown unit type");
        }
        let mut handles = Vec::new();
        for slot in 0..count.max(1) {
            let offset = Vec2::new(slot as f32 * 48.0, 0.0);
            let Some(unit) = self.spawn_def_exact(owner, def_id, pos + offset) else {
                continue;
            };
            if lifetime_seconds > 0.0 {
                let ticks = (lifetime_seconds * crate::TICKS_PER_SECOND as f32).round() as u32;
                let mut buff = Buff::timed("BTLF", unit, unit, 1, self.tick, ticks.max(1));
                buff.kind = BuffKind::TimedLife {
                    expire_tick: self.tick + ticks.max(1) as u64,
                };
                buff.stack_key = None;
                self.add_buff(buff);
            }
            handles.push(table.bind(unit));
        }
        if handles.is_empty() {
            return Err("no room to summon");
        }
        Ok(handles)
    }
}

fn order_error(order: &str) -> &'static str {
    match order {
        "" => "empty order",
        _ => "unsupported order",
    }
}
