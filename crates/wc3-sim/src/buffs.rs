//! Buff / aura / timed-life system.
//!
//! Ported mechanisms (file:line in the donors, access date 2026-11-14):
//!
//! * `CBuffTimed` — `donors/WarsmashModEngine/core/src/com/etheller/warsmash/
//!   viewer5/handlers/w3x/simulation/abilities/skills/util/CBuffTimed.java:24-67`:
//!   a buff stores `expireTick = now + duration/step`, applies its stat change
//!   in `onAdd`, reverts it in `onRemove`, and removes itself on expiry.
//! * `CAbilityAuraBase` — `.../skills/util/CAbilityAuraBase.java:16-84`: the
//!   aura source re-checks a 2.00 s area period (`AURA_PERIODIC_CHECK_TIME`),
//!   adds the aura buff to every valid target and replaces a lower-level buff
//!   of the same id with the higher-level one.
//! * `CBuffAuraBase` — `.../skills/util/CBuffAuraBase.java:18-88`: the buff on
//!   the *target* re-checks every 2.00 s whether the source can still reach it
//!   (`canReach(unit, areaOfEffect)`) and removes itself when it cannot.
//! * `NonStackingStatBuff` — `.../simulation/unit/NonStackingStatBuff.java:9-27`:
//!   a stat buff carries a `stackingKey`; buffs that share a key do not stack,
//!   only the strongest value survives.  `ALLOW_STACKING_KEY = "STACK"` opts a
//!   buff out of the rule.
//!
//! Effects are resolved through [`Sim::effective_*`] accessors, so base unit
//! stats are never mutated by a buff and `state_hash` stays reproducible.

use glam::Vec2;

use crate::{Sim, TICKS_PER_SECOND, UnitId};

/// Process-wide interner for buff tags.
///
/// A save stores tags as text (they are data-driven), while the simulation
/// keeps `&'static str` so `Buff` stays `Copy` and the state hash never depends
/// on allocation.  Each distinct tag is leaked once per process; the set is
/// bounded by the abilities a session actually uses.
pub fn intern_tag(tag: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Mutex::new(HashSet::new()));
    let mut pool = pool.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(existing) = pool.get(tag) {
        return existing;
    }
    let leaked: &'static str = Box::leak(tag.to_string().into_boxed_str());
    pool.insert(leaked);
    leaked
}

/// Aura re-check period in seconds (`AURA_PERIODIC_CHECK_TIME`).
pub const AURA_PERIOD_SECONDS: f32 = 2.0;

/// Aura re-check period in ticks at the simulation rate.
pub fn aura_period_ticks() -> u64 {
    (AURA_PERIOD_SECONDS * TICKS_PER_SECOND as f32).round() as u64
}

/// Which derived stat a [`StatMod`] touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatKey {
    /// Flat armor points (negative for armor reduction).
    Armor,
    /// Attack damage, permille of the base damage (1000 = +100 %).
    AttackDamagePermille,
    /// Attack interval, permille (1000 = no change, 500 = twice as fast).
    AttackSpeedPermille,
    /// Movement speed, permille of the base speed.
    MoveSpeedPermille,
    /// Incoming damage, permille (1000 = no change, 500 = takes half).
    DamageTakenPermille,
    /// Bonus maximum life.
    MaxLife,
}

/// One stat change carried by a buff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatMod {
    pub key: StatKey,
    pub value: i32,
}

impl StatMod {
    pub const fn new(key: StatKey, value: i32) -> Self {
        Self { key, value }
    }

    pub const fn armor(value: i32) -> Self {
        Self::new(StatKey::Armor, value)
    }

    pub const fn damage_permille(value: i32) -> Self {
        Self::new(StatKey::AttackDamagePermille, value)
    }

    pub const fn attack_speed_permille(value: i32) -> Self {
        Self::new(StatKey::AttackSpeedPermille, value)
    }

    pub const fn move_speed_permille(value: i32) -> Self {
        Self::new(StatKey::MoveSpeedPermille, value)
    }

    pub const fn damage_taken_permille(value: i32) -> Self {
        Self::new(StatKey::DamageTakenPermille, value)
    }
}

/// How long a buff lives and how it is refreshed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuffKind {
    /// Ordinary timed buff; `expire_tick` is the tick the buff is removed.
    Timed { expire_tick: u64 },
    /// Summon lifetime buff: expiry despawns the unit (`CBuffTimedLife`).
    TimedLife { expire_tick: u64 },
    /// Aura emitter on the source unit; `CBuffAuraBase`-style refresh.
    AuraSource {
        radius: f32,
        hostile: bool,
        next_refresh: u64,
    },
    /// Aura receiver on a target unit; drops when the source is out of reach.
    AuraTarget {
        source: UnitId,
        radius: f32,
        next_refresh: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuffId(pub u32);

/// A live buff instance.  `Vec`-free so the save format stays simple and the
/// state hash cannot depend on allocation order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Buff {
    pub id: BuffId,
    /// Buff rawcode/id (`BUau`, `BHab`, `BHds`, ...); also the aura join key.
    pub tag: &'static str,
    /// Unit that carries the buff.
    pub unit: UnitId,
    /// Unit that applied it (the aura source for aura buffs).
    pub source: UnitId,
    pub level: u8,
    pub kind: BuffKind,
    pub armor: i32,
    pub damage_permille: i32,
    pub attack_speed_permille: i32,
    pub move_speed_permille: i32,
    pub damage_taken_permille: i32,
    pub bonus_life: i32,
    /// Movement is blocked while the buff lives (Entangling Roots).
    pub root: bool,
    /// Attack/move are blocked while the buff lives (stun family).
    pub stun: bool,
    /// Damage is ignored while the buff lives (Divine Shield).
    pub invulnerable: bool,
    /// Non-stacking key; `None` means "always stacks" (`STACK`).
    pub stack_key: Option<&'static str>,
    /// Strength used to resolve same-key conflicts (higher wins).
    pub stack_prio: i32,
}

impl Buff {
    /// Builds a timed buff with no stat changes (base for the helpers below).
    pub fn timed(
        tag: &'static str,
        unit: UnitId,
        source: UnitId,
        level: u8,
        now: u64,
        duration_ticks: u32,
    ) -> Self {
        Self {
            id: BuffId(0),
            tag,
            unit,
            source,
            level,
            kind: BuffKind::Timed {
                expire_tick: now + duration_ticks.max(1) as u64,
            },
            armor: 0,
            damage_permille: 0,
            attack_speed_permille: 0,
            move_speed_permille: 0,
            damage_taken_permille: 0,
            bonus_life: 0,
            root: false,
            stun: false,
            invulnerable: false,
            stack_key: Some(tag),
            stack_prio: level as i32,
        }
    }

    pub fn with_armor(mut self, armor: i32) -> Self {
        self.armor = armor;
        self
    }

    pub fn with_damage_permille(mut self, permille: i32) -> Self {
        self.damage_permille = permille;
        self
    }

    pub fn with_attack_speed_permille(mut self, permille: i32) -> Self {
        self.attack_speed_permille = permille;
        self
    }

    pub fn with_move_speed_permille(mut self, permille: i32) -> Self {
        self.move_speed_permille = permille;
        self
    }

    pub fn with_damage_taken_permille(mut self, permille: i32) -> Self {
        self.damage_taken_permille = permille;
        self
    }

    pub fn rooted(mut self) -> Self {
        self.root = true;
        self
    }

    pub fn stunned(mut self) -> Self {
        self.stun = true;
        self
    }

    pub fn invulnerable(mut self) -> Self {
        self.invulnerable = true;
        self
    }

    /// Marks the buff as always stackable (`NonStackingStatBuff.ALLOW_STACKING_KEY`).
    pub fn stacking(mut self) -> Self {
        self.stack_key = None;
        self
    }

    pub fn stack_key(mut self, key: &'static str) -> Self {
        self.stack_key = Some(key);
        self
    }

    pub fn stack_prio(mut self, prio: i32) -> Self {
        self.stack_prio = prio;
        self
    }

    /// Remaining duration in ticks for timed buffs (`0` for auras).
    pub fn remaining_ticks(&self, now: u64) -> u64 {
        match self.kind {
            BuffKind::Timed { expire_tick } | BuffKind::TimedLife { expire_tick } => {
                expire_tick.saturating_sub(now)
            }
            _ => 0,
        }
    }

    pub fn is_expired(&self, now: u64) -> bool {
        match self.kind {
            BuffKind::Timed { expire_tick } | BuffKind::TimedLife { expire_tick } => {
                now >= expire_tick
            }
            _ => false,
        }
    }
}

/// Outcome of [`Sim::add_buff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffApplied {
    /// Buff inserted with this id.
    Added(BuffId),
    /// A same-key buff of equal or higher strength was already present.
    Rejected,
    /// The target unit does not exist or is dead.
    NoTarget,
}

impl Sim {
    /// Inserts a buff on `buff.unit` and resolves stacking.
    ///
    /// Stacking rule (port of `NonStackingStatBuff`): when the incoming buff has
    /// a `stack_key` and an existing buff on the same unit shares it, the buff
    /// with the lower `stack_prio` is removed; equal strength keeps the older
    /// buff.  Aura buffs additionally follow the `CAbilityAuraBase` rule: a
    /// higher-level buff of the same tag replaces the lower-level one.
    pub fn add_buff(&mut self, mut buff: Buff) -> BuffApplied {
        let Some(unit) = self.unit(buff.unit) else {
            return BuffApplied::NoTarget;
        };
        if !unit.alive {
            return BuffApplied::NoTarget;
        }
        if let Some(key) = buff.stack_key {
            let existing: Vec<(BuffId, i32)> = self
                .buffs
                .iter()
                .filter(|held| held.unit == buff.unit && held.stack_key == Some(key))
                .map(|held| (held.id, held.stack_prio))
                .collect();
            for (id, prio) in existing {
                if prio >= buff.stack_prio {
                    return BuffApplied::Rejected;
                }
                self.remove_buff(buff.unit, id);
            }
        }
        // Same-tag aura replacement regardless of stacking key.
        let same_tag: Vec<(BuffId, u8)> = self
            .buffs
            .iter()
            .filter(|held| {
                held.unit == buff.unit
                    && held.tag == buff.tag
                    && matches!(held.kind, BuffKind::AuraTarget { .. })
            })
            .map(|held| (held.id, held.level))
            .collect();
        for (id, level) in same_tag {
            if level <= buff.level {
                self.remove_buff(buff.unit, id);
            } else {
                return BuffApplied::Rejected;
            }
        }
        buff.id = BuffId(self.next_buff_id.max(1));
        self.next_buff_id = buff.id.0.wrapping_add(1).max(1);
        if buff.invulnerable {
            // Damage is ignored while the buff lives; the timed field keeps
            // existing `invulnerable_until` readers correct as well.
            if let BuffKind::Timed { expire_tick } | BuffKind::TimedLife { expire_tick } = buff.kind
                && let Some(index) = self.unit_index(buff.unit)
            {
                let unit = &mut self.units[index];
                if expire_tick > unit.invulnerable_until {
                    unit.invulnerable_until = expire_tick;
                }
            }
        }
        if buff.bonus_life != 0
            && let Some(index) = self.unit_index(buff.unit)
        {
            let unit = &mut self.units[index];
            unit.max_hp = (unit.max_hp + buff.bonus_life).max(1);
            unit.hp = (unit.hp + buff.bonus_life).clamp(1, unit.max_hp);
        }
        if buff.stun
            && let BuffKind::Timed { expire_tick } | BuffKind::TimedLife { expire_tick } = buff.kind
            && let Some(index) = self.unit_index(buff.unit)
        {
            let unit = &mut self.units[index];
            if expire_tick > unit.stunned_until {
                unit.stunned_until = expire_tick;
            }
        }
        if buff.root
            && let BuffKind::Timed { expire_tick } | BuffKind::TimedLife { expire_tick } = buff.kind
            && let Some(index) = self.unit_index(buff.unit)
        {
            let unit = &mut self.units[index];
            if expire_tick > unit.rooted_until {
                unit.rooted_until = expire_tick;
            }
        }
        let id = buff.id;
        let pos = self
            .buffs
            .binary_search_by(|held| (held.unit.0, held.id.0).cmp(&(buff.unit.0, buff.id.0)))
            .unwrap_or_else(|pos| pos);
        self.buffs.insert(pos, buff);
        BuffApplied::Added(id)
    }

    /// Removes one buff and reverts its non-derived effects (bonus life).
    pub fn remove_buff(&mut self, unit: UnitId, id: BuffId) -> bool {
        let Some(index) = self
            .buffs
            .iter()
            .position(|buff| buff.unit == unit && buff.id == id)
        else {
            return false;
        };
        let buff = self.buffs.remove(index);
        if buff.bonus_life != 0
            && let Some(unit_index) = self.unit_index(unit)
        {
            let target = &mut self.units[unit_index];
            target.max_hp = (target.max_hp - buff.bonus_life).max(1);
            target.hp = target.hp.min(target.max_hp);
        }
        true
    }

    /// Removes every buff with `tag` from `unit`.
    pub fn remove_buffs_by_tag(&mut self, unit: UnitId, tag: &str) -> usize {
        let ids: Vec<BuffId> = self
            .buffs
            .iter()
            .filter(|buff| buff.unit == unit && buff.tag == tag)
            .map(|buff| buff.id)
            .collect();
        let removed = ids.len();
        for id in ids {
            self.remove_buff(unit, id);
        }
        removed
    }

    /// Removes every buff on a unit (death cleanup).
    pub fn clear_buffs(&mut self, unit: UnitId) -> usize {
        let before = self.buffs.len();
        self.buffs.retain(|buff| buff.unit != unit);
        before - self.buffs.len()
    }

    /// Buffs currently living on `unit`, in id order.
    pub fn buffs_of(&self, unit: UnitId) -> impl Iterator<Item = &Buff> {
        self.buffs.iter().filter(move |buff| buff.unit == unit)
    }

    pub fn buff_count(&self, unit: UnitId) -> usize {
        self.buffs.iter().filter(|buff| buff.unit == unit).count()
    }

    pub fn has_buff(&self, unit: UnitId, tag: &str) -> bool {
        self.buffs
            .iter()
            .any(|buff| buff.unit == unit && buff.tag == tag)
    }

    /// Effective armor: base plus every buff contribution.
    pub fn effective_armor(&self, unit: UnitId) -> i32 {
        let base = self.unit(unit).map(|unit| unit.armor).unwrap_or(0);
        if self.buffs.is_empty() {
            return base;
        }
        self.buffs
            .iter()
            .filter(|buff| buff.unit == unit)
            .fold(base, |total, buff| total + buff.armor)
    }

    /// Attack damage multiplier in permille (1000 = unchanged).
    fn damage_permille(&self, unit: UnitId) -> i32 {
        if self.buffs.is_empty() {
            return 1000;
        }
        self.buffs
            .iter()
            .filter(|buff| buff.unit == unit)
            .fold(1000, |total, buff| total + buff.damage_permille)
            .max(1)
    }

    /// Effective per-hit damage for a base value.
    pub fn effective_damage(&self, unit: UnitId, base: i32) -> i32 {
        if base <= 0 {
            return base;
        }
        (base * self.damage_permille(unit) / 1000).max(1)
    }

    /// Effective attack interval in ticks.
    pub fn effective_attack_ticks(&self, unit: UnitId) -> u32 {
        let base = self.unit(unit).map(|unit| unit.attack_ticks).unwrap_or(1);
        let upgrade = self.attack_speed_permille_for(unit) as i64;
        let permille = self
            .buffs
            .iter()
            .filter(|buff| buff.unit == unit)
            .fold(1000i64 + upgrade, |total, buff| {
                total + buff.attack_speed_permille as i64
            })
            .clamp(100, 10_000);
        if permille == 1000 {
            return base.max(1);
        }
        ((base as i64 * 1000 / permille).max(1)) as u32
    }

    /// Effective movement speed in world units per second.
    pub fn effective_speed(&self, unit: UnitId) -> f32 {
        let base = self.unit(unit).map(|unit| unit.speed).unwrap_or(0.0);
        if self.buffs.is_empty() {
            return base;
        }
        let permille = self
            .buffs
            .iter()
            .filter(|buff| buff.unit == unit)
            .fold(1000i64, |total, buff| {
                total + buff.move_speed_permille as i64
            })
            .clamp(50, 5000);
        base * permille as f32 / 1000.0
    }

    /// Incoming-damage multiplier in permille (1000 = unchanged).
    pub fn damage_taken_permille(&self, unit: UnitId) -> i32 {
        if self.buffs.is_empty() {
            return 1000;
        }
        self.buffs
            .iter()
            .filter(|buff| buff.unit == unit)
            .fold(1000, |total, buff| total + buff.damage_taken_permille)
            .max(1)
    }

    /// True when a buff (or the legacy order field) blocks movement.
    pub fn is_rooted(&self, unit: UnitId) -> bool {
        let legacy = self
            .unit(unit)
            .map(|unit| unit.rooted_until > self.tick)
            .unwrap_or(false);
        legacy || self.buffs.iter().any(|buff| buff.unit == unit && buff.root)
    }

    /// True when a buff (or the legacy order field) blocks acting.
    pub fn is_stunned(&self, unit: UnitId) -> bool {
        let legacy = self
            .unit(unit)
            .map(|unit| unit.stunned_until > self.tick)
            .unwrap_or(false);
        legacy || self.buffs.iter().any(|buff| buff.unit == unit && buff.stun)
    }

    /// True when damage must be ignored entirely.
    pub fn is_buff_invulnerable(&self, unit: UnitId) -> bool {
        self.buffs
            .iter()
            .any(|buff| buff.unit == unit && buff.invulnerable)
    }

    /// Advances every buff: expiry, timed-life despawn, aura refresh/drop.
    ///
    /// Fixed iteration order (id order) so the result is deterministic.
    pub fn tick_buffs(&mut self) {
        if self.buffs.is_empty() {
            return;
        }
        let now = self.tick;
        let period = aura_period_ticks();

        // Aura emitters re-apply their buff to every valid target.
        let emitters: Vec<(UnitId, UnitId, &'static str, u8, f32, bool, u64)> = self
            .buffs
            .iter()
            .filter_map(|buff| match buff.kind {
                BuffKind::AuraSource {
                    radius,
                    hostile,
                    next_refresh,
                } if now >= next_refresh => Some((
                    buff.unit,
                    buff.source,
                    buff.tag,
                    buff.level,
                    radius,
                    hostile,
                    buff.id.0 as u64,
                )),
                _ => None,
            })
            .collect();
        for (source, _stamp, tag, level, radius, hostile, id) in emitters {
            if let Some(index) = self
                .buffs
                .iter()
                .position(|buff| buff.id.0 as u64 == id && buff.unit == source)
            {
                if let BuffKind::AuraSource { next_refresh, .. } = &mut self.buffs[index].kind {
                    *next_refresh = now + period;
                }
            }
            let Some(source_unit) = self.unit(source) else {
                continue;
            };
            if !source_unit.alive {
                continue;
            }
            let centre = source_unit.pos;
            let owner = source_unit.owner;
            let template: Vec<Buff> = self
                .buffs
                .iter()
                .filter(|buff| {
                    buff.unit == source
                        && buff.tag == tag
                        && matches!(buff.kind, BuffKind::AuraSource { .. })
                })
                .map(|buff| *buff)
                .collect();
            let Some(template) = template.first().copied() else {
                continue;
            };
            let mut targets: Vec<UnitId> = self
                .units
                .iter()
                .filter(|unit| {
                    unit.alive
                        && unit.id != source
                        && unit.pos.distance(centre) <= radius + unit.radius()
                        && if hostile {
                            unit.owner != owner
                        } else {
                            unit.owner == owner
                        }
                })
                .map(|unit| unit.id)
                .collect();
            targets.sort();
            for target in targets {
                let mut buff = template;
                buff.id = BuffId(0);
                buff.unit = target;
                buff.kind = BuffKind::AuraTarget {
                    source,
                    radius,
                    next_refresh: now + period,
                };
                buff.stack_key = Some(tag);
                buff.stack_prio = level as i32;
                buff.invulnerable = false;
                buff.bonus_life = 0;
                buff.stun = false;
                buff.root = false;
                if self.has_buff(target, tag) {
                    // Refresh the existing aura buff instead of stacking it.
                    if let Some(index) = self
                        .buffs
                        .iter()
                        .position(|held| held.unit == target && held.tag == tag)
                    {
                        if let BuffKind::AuraTarget { next_refresh, .. } =
                            &mut self.buffs[index].kind
                        {
                            *next_refresh = now + period;
                        }
                    }
                    continue;
                }
                let _ = self.add_buff(buff);
            }
        }

        // Expiry and aura drop, collected first so removal order is stable.
        let mut drop: Vec<(UnitId, BuffId, bool)> = Vec::new();
        let mut expired_auras: Vec<(UnitId, BuffId)> = Vec::new();
        for buff in &self.buffs {
            match buff.kind {
                BuffKind::Timed { expire_tick } if now >= expire_tick => {
                    drop.push((buff.unit, buff.id, false));
                }
                BuffKind::TimedLife { expire_tick } if now >= expire_tick => {
                    drop.push((buff.unit, buff.id, true));
                }
                BuffKind::AuraTarget {
                    source,
                    radius,
                    next_refresh,
                } if now >= next_refresh => {
                    let alive_source = self.unit(source).map(|unit| unit.alive).unwrap_or(false);
                    let reachable = alive_source
                        && self
                            .unit(buff.unit)
                            .map(|unit| {
                                self.unit(source)
                                    .map(|source_unit| {
                                        source_unit.pos.distance(unit.pos) <= radius + unit.radius()
                                    })
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);
                    if reachable {
                        expired_auras.push((buff.unit, buff.id));
                    } else {
                        drop.push((buff.unit, buff.id, false));
                    }
                }
                _ => {}
            }
        }
        for (unit, id) in expired_auras {
            if let Some(index) = self
                .buffs
                .iter()
                .position(|buff| buff.unit == unit && buff.id == id)
                && let BuffKind::AuraTarget { next_refresh, .. } = &mut self.buffs[index].kind
            {
                *next_refresh = now + period;
            }
        }
        for (unit, id, despawn) in drop {
            self.remove_buff(unit, id);
            if despawn {
                self.despawn_summon(unit);
            }
        }
    }

    /// Kills a summon whose timed life ran out.  WC3 grants no experience for an
    /// expired summon, so the plain [`Sim::kill_unit`] path is used and the
    /// summon's own buffs are dropped.
    fn despawn_summon(&mut self, unit: UnitId) {
        if !self.unit(unit).map(|unit| unit.alive).unwrap_or(false) {
            return;
        }
        self.kill_unit(unit);
        self.clear_buffs(unit);
    }

    /// Adds an aura emitter to `source` (idempotent per tag).
    pub fn add_aura(
        &mut self,
        source: UnitId,
        tag: &'static str,
        level: u8,
        radius: f32,
        hostile: bool,
        template: Buff,
    ) -> BuffApplied {
        if self.has_buff(source, tag) {
            return BuffApplied::Rejected;
        }
        let mut buff = template;
        buff.id = BuffId(0);
        buff.tag = tag;
        buff.unit = source;
        buff.source = source;
        buff.level = level;
        buff.kind = BuffKind::AuraSource {
            radius,
            hostile,
            next_refresh: self.tick,
        };
        self.add_buff(buff)
    }

    /// Position helper used by tests and effects: units within `radius` of
    /// `centre` that are alive and satisfy `filter`, sorted by id.
    pub fn units_in_radius(
        &self,
        centre: Vec2,
        radius: f32,
        filter: impl Fn(&crate::Unit) -> bool,
    ) -> Vec<UnitId> {
        let mut ids: Vec<UnitId> = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive && unit.pos.distance(centre) <= radius + unit.radius() && filter(unit)
            })
            .map(|unit| unit.id)
            .collect();
        ids.sort();
        ids
    }
}
