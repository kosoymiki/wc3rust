//! Generic ability effect runner.
//!
//! The donor engines each implement abilities as small handler classes.  The
//! taxonomy below is a direct port of the Warsmash handler set, which lets one
//! runner execute every data-driven ability instead of hand-writing each one:
//!
//! | handler | donor evidence (access date 2026-11-14) |
//! |---|---|
//! | damage | `.../abilities/skills/human/paladin/CAbilityHolyLight.java:52-104` (friendly heal, undead damage) |
//! | heal | `.../abilities/skills/human/paladin/CAbilityHolyLight.java:52-104` |
//! | buff | `.../abilities/skills/util/CBuffTimed.java:24-67` + `CAbilityDivineShield.java:18-46` |
//! | aura | `.../abilities/skills/util/CAbilityAuraBase.java:16-84`, `CAbilityDevotion.java:14-40` |
//! | summon | `.../abilities/skills/human/archmage/CAbilitySummonWaterElemental.java:41-63` |
//! | movement | `.../abilities/skills/neutral/...` slow/root buffs (`CBuffSlow.java:12-40`) |
//!
//! Every handler mutates the canonical [`crate::Sim`] only through its public
//! surface, so an ability produces the same events, XP and death handling as a
//! player-issued command.

use glam::Vec2;

use crate::buffs::{Buff, BuffApplied, StatMod};
use crate::data::AttackType;
use crate::{Event, Sim, UnitId};

/// One effect a handler can execute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectSpec {
    /// Single-target typed damage.
    Damage { amount: i32, attack: AttackType },
    /// Damage every valid unit inside `radius` of the effect position.
    DamageArea {
        radius: f32,
        amount: i32,
        attack: AttackType,
        hostile_only: bool,
    },
    /// Heal an ally or damage an enemy (Death Coil / Holy Light family).
    HealOrDamage { amount: i32, attack: AttackType },
    /// Single-target heal.
    Heal { amount: i32 },
    /// Heal every valid unit inside `radius` of the effect position.
    HealArea {
        radius: f32,
        amount: i32,
        allies_only: bool,
    },
    /// Timed buff on the effect target (or the caster for self-cast).
    Buff {
        tag: &'static str,
        duration_ticks: u32,
        mods: &'static [StatMod],
        root: bool,
        stun: bool,
        invulnerable: bool,
        level: u8,
    },
    /// Aura emitter installed on the caster.
    Aura {
        tag: &'static str,
        radius: f32,
        level: u8,
        mods: &'static [StatMod],
        move_speed_permille: i32,
        hostile: bool,
    },
    /// Summon `count` units of `def_id` in front of the caster with a timed life.
    Summon {
        def_id: &'static str,
        count: u8,
        lifetime_ticks: u32,
        forward: f32,
    },
    /// Movement modifier applied to the effect target.
    Movement {
        slow_permille: i32,
        root: bool,
        duration_ticks: u32,
        tag: &'static str,
    },
}

/// What one handler did, for events, tests and honest "could not apply" reports.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectOutcome {
    Damaged {
        unit: UnitId,
        amount: i32,
    },
    Healed {
        unit: UnitId,
        amount: i32,
    },
    Buffed {
        unit: UnitId,
        buff: u32,
        tag: &'static str,
    },
    Summoned {
        unit: UnitId,
        def_id: &'static str,
    },
    Slowed {
        unit: UnitId,
        permille: i32,
    },
    Rooted {
        unit: UnitId,
    },
    /// Aura installed on the caster.
    AuraInstalled {
        unit: UnitId,
        tag: &'static str,
    },
    /// Handler recognised but could not run (missing target, unknown unit type).
    Skipped {
        reason: &'static str,
    },
}

/// Resolved targets for one cast.
#[derive(Debug, Clone, Copy)]
pub struct EffectContext {
    pub caster: UnitId,
    pub target: Option<UnitId>,
    pub point: Option<Vec2>,
}

impl EffectContext {
    pub fn new(caster: UnitId) -> Self {
        Self {
            caster,
            target: None,
            point: None,
        }
    }

    pub fn with_target(mut self, target: UnitId) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_point(mut self, point: Vec2) -> Self {
        self.point = Some(point);
        self
    }
}

/// Applies a list of handlers in order.  The order is the data order, so a
/// damage-then-heal ability is reproducible.
pub fn run_effects(
    sim: &mut Sim,
    ctx: EffectContext,
    effects: &[EffectSpec],
) -> Vec<EffectOutcome> {
    let mut out = Vec::with_capacity(effects.len());
    for effect in effects {
        out.push(run_effect(sim, ctx, *effect));
    }
    out
}

/// Executes one handler.
pub fn run_effect(sim: &mut Sim, ctx: EffectContext, effect: EffectSpec) -> EffectOutcome {
    let Some(caster) = sim.unit(ctx.caster) else {
        return EffectOutcome::Skipped {
            reason: "no caster",
        };
    };
    if !caster.alive {
        return EffectOutcome::Skipped {
            reason: "caster dead",
        };
    }
    let caster_owner = caster.owner;
    let caster_pos = caster.pos;
    let caster_facing = caster.facing;
    match effect {
        EffectSpec::Damage { amount, attack } => {
            let Some(target) = ctx.target else {
                return EffectOutcome::Skipped {
                    reason: "no target",
                };
            };
            if sim
                .unit(target)
                .map(|unit| unit.owner == caster_owner)
                .unwrap_or(false)
            {
                return EffectOutcome::Skipped {
                    reason: "target is friendly",
                };
            }
            let dealt = sim.apply_damage(ctx.caster, target, amount, attack);
            EffectOutcome::Damaged {
                unit: target,
                amount: dealt,
            }
        }
        EffectSpec::DamageArea {
            radius,
            amount,
            attack,
            hostile_only,
        } => {
            let centre = ctx.point.unwrap_or(caster_pos);
            let mut targets = sim.units_in_radius(centre, radius, |unit| {
                if hostile_only {
                    unit.owner != caster_owner
                } else {
                    true
                }
            });
            // The primary target is damaged first when it is inside the area.
            if let Some(primary) = ctx.target
                && let Some(pos) = targets.iter().position(|id| *id == primary)
            {
                targets.swap(0, pos);
            }
            let mut first = None;
            let mut total = 0;
            for target in targets {
                if target == ctx.caster {
                    continue;
                }
                let dealt = sim.apply_damage(ctx.caster, target, amount, attack);
                total += dealt;
                if first.is_none() {
                    first = Some(target);
                }
            }
            match first {
                Some(unit) => EffectOutcome::Damaged {
                    unit,
                    amount: total,
                },
                None => EffectOutcome::Skipped {
                    reason: "no units in area",
                },
            }
        }
        EffectSpec::HealOrDamage { amount, attack } => {
            let Some(target) = ctx.target else {
                return EffectOutcome::Skipped {
                    reason: "no target",
                };
            };
            let ally = sim
                .unit(target)
                .map(|unit| unit.owner == caster_owner)
                .unwrap_or(false);
            if ally {
                let healed = sim.heal_unit(target, amount);
                EffectOutcome::Healed {
                    unit: target,
                    amount: healed,
                }
            } else {
                let dealt = sim.apply_damage(ctx.caster, target, amount, attack);
                EffectOutcome::Damaged {
                    unit: target,
                    amount: dealt,
                }
            }
        }
        EffectSpec::Heal { amount } => {
            let Some(target) = ctx.target else {
                return EffectOutcome::Skipped {
                    reason: "no target",
                };
            };
            let healed = sim.heal_unit(target, amount);
            EffectOutcome::Healed {
                unit: target,
                amount: healed,
            }
        }
        EffectSpec::HealArea {
            radius,
            amount,
            allies_only,
        } => {
            let centre = ctx.point.unwrap_or(caster_pos);
            let targets = sim.units_in_radius(centre, radius, |unit| {
                if allies_only {
                    unit.owner == caster_owner
                } else {
                    true
                }
            });
            let mut first = None;
            let mut total = 0;
            for target in targets {
                let healed = sim.heal_unit(target, amount);
                total += healed;
                if first.is_none() && healed > 0 {
                    first = Some(target);
                }
            }
            match first {
                Some(unit) => EffectOutcome::Healed {
                    unit,
                    amount: total,
                },
                None => EffectOutcome::Skipped {
                    reason: "no units in area",
                },
            }
        }
        EffectSpec::Buff {
            tag,
            duration_ticks,
            mods,
            root,
            stun,
            invulnerable,
            level,
        } => {
            // Self-cast abilities (Divine Shield, Avatar) carry no target.
            let target = ctx.target.unwrap_or(ctx.caster);
            let mut buff = Buff::timed(tag, target, ctx.caster, level, sim.tick, duration_ticks);
            buff.root = root;
            buff.stun = stun;
            buff.invulnerable = invulnerable;
            for stat in mods {
                match stat.key {
                    crate::buffs::StatKey::Armor => buff.armor += stat.value,
                    crate::buffs::StatKey::AttackDamagePermille => {
                        buff.damage_permille += stat.value
                    }
                    crate::buffs::StatKey::AttackSpeedPermille => {
                        buff.attack_speed_permille += stat.value
                    }
                    crate::buffs::StatKey::MoveSpeedPermille => {
                        buff.move_speed_permille += stat.value
                    }
                    crate::buffs::StatKey::DamageTakenPermille => {
                        buff.damage_taken_permille += stat.value
                    }
                    crate::buffs::StatKey::MaxLife => buff.bonus_life += stat.value,
                }
            }
            match sim.add_buff(buff) {
                BuffApplied::Added(id) => EffectOutcome::Buffed {
                    unit: target,
                    buff: id.0,
                    tag,
                },
                BuffApplied::Rejected => EffectOutcome::Skipped {
                    reason: "stronger buff already present",
                },
                BuffApplied::NoTarget => EffectOutcome::Skipped {
                    reason: "no target",
                },
            }
        }
        EffectSpec::Aura {
            tag,
            radius,
            level,
            mods,
            move_speed_permille,
            hostile,
        } => {
            let mut template = Buff::timed(tag, ctx.caster, ctx.caster, level, sim.tick, 1);
            for stat in mods {
                match stat.key {
                    crate::buffs::StatKey::Armor => template.armor += stat.value,
                    crate::buffs::StatKey::AttackDamagePermille => {
                        template.damage_permille += stat.value
                    }
                    crate::buffs::StatKey::AttackSpeedPermille => {
                        template.attack_speed_permille += stat.value
                    }
                    crate::buffs::StatKey::MoveSpeedPermille => {
                        template.move_speed_permille += stat.value
                    }
                    crate::buffs::StatKey::DamageTakenPermille => {
                        template.damage_taken_permille += stat.value
                    }
                    crate::buffs::StatKey::MaxLife => template.bonus_life += stat.value,
                }
            }
            template.move_speed_permille += move_speed_permille;
            // Aura buffs are refreshed by the emitter, never by a timer.
            template.kind = crate::buffs::BuffKind::AuraSource {
                radius,
                hostile,
                next_refresh: sim.tick,
            };
            match sim.add_aura(ctx.caster, tag, level, radius, hostile, template) {
                BuffApplied::Added(_) => EffectOutcome::AuraInstalled {
                    unit: ctx.caster,
                    tag,
                },
                BuffApplied::Rejected => EffectOutcome::Skipped {
                    reason: "aura already active",
                },
                BuffApplied::NoTarget => EffectOutcome::Skipped {
                    reason: "no caster",
                },
            }
        }
        EffectSpec::Summon {
            def_id,
            count,
            lifetime_ticks,
            forward,
        } => {
            if crate::def_of(def_id).is_none() {
                return EffectOutcome::Skipped {
                    reason: "summon unit type not in roster",
                };
            }
            let angle = caster_facing;
            let mut first = None;
            for slot in 0..count.max(1) {
                let spread = (slot as f32 - (count.max(1) as f32 - 1.0) * 0.5) * 90.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let side = Vec2::new(-dir.y, dir.x);
                let wanted = caster_pos + dir * forward.max(24.0) + side * spread;
                let pos = sim
                    .nearest_walkable_cell(sim.grid.world_to_cell(wanted), 12)
                    .map(|cell| sim.grid.cell_to_world(cell.0, cell.1))
                    .unwrap_or(caster_pos);
                let Some(unit) = sim.spawn_def_exact(caster_owner, def_id, pos) else {
                    continue;
                };
                if lifetime_ticks > 0 {
                    let mut life =
                        Buff::timed("BTLF", unit, ctx.caster, 1, sim.tick, lifetime_ticks);
                    life.kind = crate::buffs::BuffKind::TimedLife {
                        expire_tick: sim.tick + lifetime_ticks.max(1) as u64,
                    };
                    life.stack_key = None;
                    let _ = sim.add_buff(life);
                }
                if first.is_none() {
                    first = Some(unit);
                }
            }
            match first {
                Some(unit) => EffectOutcome::Summoned { unit, def_id },
                None => EffectOutcome::Skipped {
                    reason: "no room to summon",
                },
            }
        }
        EffectSpec::Movement {
            slow_permille,
            root,
            duration_ticks,
            tag,
        } => {
            let Some(target) = ctx.target else {
                return EffectOutcome::Skipped {
                    reason: "no target",
                };
            };
            let mut buff = Buff::timed(tag, target, ctx.caster, 1, sim.tick, duration_ticks);
            buff.move_speed_permille = -slow_permille.abs();
            buff.root = root;
            buff.stack_key = Some(tag);
            let root_outcome = root;
            match sim.add_buff(buff) {
                BuffApplied::Added(_) => {
                    if root_outcome {
                        EffectOutcome::Rooted { unit: target }
                    } else {
                        EffectOutcome::Slowed {
                            unit: target,
                            permille: slow_permille.abs(),
                        }
                    }
                }
                BuffApplied::Rejected => EffectOutcome::Skipped {
                    reason: "stronger slow already present",
                },
                BuffApplied::NoTarget => EffectOutcome::Skipped {
                    reason: "no target",
                },
            }
        }
    }
}

/// Damage type used by the "undead only" branch of Holy Light / Death Coil.
pub const HOLY_DAMAGE: i32 = 100;

/// Pushes the standard ability events for an outcome list.  The caster is a
/// separate argument because a handler outcome only names the affected unit.
pub fn emit_events(
    sim: &mut Sim,
    caster: UnitId,
    ability: &'static str,
    outcomes: &[EffectOutcome],
) {
    for outcome in outcomes {
        match *outcome {
            EffectOutcome::Damaged { unit, amount } if amount > 0 => {
                sim.events.push(Event::DataAbilityHit {
                    caster,
                    ability,
                    to: unit,
                    amount,
                    heal: false,
                });
            }
            EffectOutcome::Healed { unit, amount } if amount > 0 => {
                sim.events.push(Event::DataAbilityHit {
                    caster,
                    ability,
                    to: unit,
                    amount,
                    heal: true,
                });
            }
            _ => {}
        }
    }
}

/// Builds the handler list for a data ability row.
///
/// The donor engines compile one handler per ability class; here the same
/// mapping is expressed as data so a new row needs no new code.  Numbers come
/// from the `AbilityData.slk` row already parsed into [`GenericAbility`].
pub fn build_effects(def: &crate::GenericAbility) -> Vec<EffectSpec> {
    let mut effects: Vec<EffectSpec> = Vec::with_capacity(2);
    match def.id {
        // Paladin Divine Shield: no-target self invulnerability (10 s, 75 s cd).
        "AHds" => {
            effects.push(EffectSpec::Buff {
                tag: "BHds",
                duration_ticks: 10 * crate::TICKS_PER_SECOND,
                mods: &[],
                root: false,
                stun: false,
                invulnerable: true,
                level: 1,
            });
        }
        // Entangling Roots: damage plus a movement block.
        "AEer" => {
            effects.push(EffectSpec::Damage {
                amount: def.power,
                attack: AttackType::Magic,
            });
            effects.push(EffectSpec::Movement {
                slow_permille: 1000,
                root: true,
                duration_ticks: def.duration_ticks.max(1),
                tag: "BEer",
            });
        }
        // Summon abilities: the unit id lives in `UnitID1`, which the roster
        // generator does not carry yet; the authentic rawcodes are listed here.
        "AHwe" => effects.push(EffectSpec::Summon {
            def_id: "hwat",
            count: def.power.max(1) as u8,
            lifetime_ticks: def.duration_ticks,
            forward: def.radius.max(64.0),
        }),
        "AOsf" => effects.push(EffectSpec::Summon {
            def_id: "osw1",
            count: def.secondary.max(1) as u8,
            lifetime_ticks: def.duration_ticks,
            forward: def.radius.max(64.0),
        }),
        "ACwe" => effects.push(EffectSpec::Summon {
            def_id: "nsel",
            count: def.power.max(1) as u8,
            lifetime_ticks: def.duration_ticks,
            forward: def.radius.max(64.0),
        }),
        // Death Coil: heals allies, damages enemies.
        "AUdc" | "AHhb" => effects.push(EffectSpec::HealOrDamage {
            amount: def.power,
            attack: AttackType::Magic,
        }),
        // Divine Shield (item, `AIdv`): 25 s self invulnerability, no stat
        // mods.  The item row carries `Dur1=25`; the generated GenericAbility
        // has duration_ticks=750 and power=0, so the generic fallback would
        // classify it as DataOnly and refuse the cast.
        "AIdv" => effects.push(EffectSpec::Buff {
            tag: "BHds",
            duration_ticks: 750,
            mods: &[],
            root: false,
            stun: false,
            invulnerable: true,
            level: 1,
        }),
        _ => match def.target {
            crate::TargetKind::Unit => {
                effects.push(EffectSpec::HealOrDamage {
                    amount: def.power,
                    attack: AttackType::Magic,
                });
            }
            crate::TargetKind::Point => {
                let amount = def.power.max(def.secondary);
                if amount > 0 && def.radius > 0.0 {
                    effects.push(EffectSpec::DamageArea {
                        radius: def.radius,
                        amount,
                        attack: AttackType::Magic,
                        hostile_only: true,
                    });
                }
            }
            crate::TargetKind::None => {}
        },
    }
    effects
}

/// Aura rows (passive, non-castable) with the effect each level applies.
///
/// Values are read from `analysis/war3_data/Units/AbilityData.slk` (access date
/// 2026-11-14): `AUau` X20/X21 = 0.10,0.50 / 0.20,1.00 / 0.30,1.50.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuraRow {
    pub id: &'static str,
    pub tag: &'static str,
    pub radius: f32,
    /// Per-level value in the aura's own unit (permille speed, flat armor).
    pub per_level: &'static [i32],
    pub kind: AuraKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraKind {
    MoveSpeedPermille,
    Armor,
    HpRegenPerSecond,
}

pub const AURAS: &[AuraRow] = &[AuraRow {
    id: "AUau",
    tag: "BUau",
    radius: 900.0,
    per_level: &[100, 200, 300],
    kind: AuraKind::MoveSpeedPermille,
}];

pub fn aura_row(id: &str) -> Option<&'static AuraRow> {
    AURAS.iter().find(|row| row.id == id)
}

impl Sim {
    /// Installs a data aura on `unit` at `level` (1-based).
    pub fn apply_data_aura(&mut self, unit: UnitId, ability_id: &str, level: u8) -> bool {
        let Some(row) = aura_row(ability_id) else {
            return false;
        };
        let Some(def) = crate::abilities::generic_ability(ability_id) else {
            return false;
        };
        if !self
            .unit(unit)
            .map(|unit| unit.learned.contains(&def.id) || unit.def().ability_ids.contains(&def.id))
            .unwrap_or(false)
        {
            return false;
        }
        let index = (level.max(1) as usize - 1).min(row.per_level.len().saturating_sub(1));
        let value = row.per_level[index];
        let mut template = Buff::timed(row.tag, unit, unit, level, self.tick, 1);
        match row.kind {
            AuraKind::MoveSpeedPermille => template.move_speed_permille = value,
            AuraKind::Armor => template.armor = value,
            AuraKind::HpRegenPerSecond => {}
        }
        matches!(
            self.add_aura(unit, row.tag, level, row.radius, false, template),
            BuffApplied::Added(_)
        )
    }
}
