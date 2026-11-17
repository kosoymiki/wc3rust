//! WC3-style ability framework: unit/point/no-target spells with mana costs,
//! cooldowns, ranges, delayed waves and deterministic targeting.
//!
//! Parameters are taken from `Units/AbilityData.slk` (level 1 columns) for the
//! implemented spells.  Effects that the original resolves through hidden
//! buffs are simplified where the simulation has no buff system yet (noted in
//! each `SpellDef`), but mana, cooldown, range and target rules are the real
//! WC3 values.

use glam::Vec2;

use crate::data::AttackType;
use crate::{Event, Sim, UnitId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Spell {
    /// Paladin AHhb: heal an ally 200 (level 1).
    HolyLight,
    /// Paladin AHds: no-target self invulnerability, 10 s, 125 mana, 75 s cd.
    DivineShield,
    /// Archmage AHbz: 6 waves of 30 magic damage in a 200 radius.
    Blizzard,
    /// Priest Ahea: 25 healing for 5 mana, 1 s cooldown.
    Heal,
    /// Keeper AEer: roots an enemy, 15 damage, immobilised for 6 s.
    EntanglingRoots,
    /// Far Seer AOcl: 100 damage then up to 4 bounces at 85% each.
    ChainLightning,
    /// Mountain King AHtb: 100 damage and a 3 s stun.
    StormBolt,
    /// Mountain King AHtc: area damage around the caster.
    ThunderClap,
    /// Death Knight AUdc: heals an ally or damages an enemy for 200.
    DeathCoil,
    /// Lich AUfn: damage the target and enemies around it.
    FrostNova,
    /// Dread Lord AUcs: area damage at a point.
    CarrionSwarm,
    /// Hero ability with a data-backed damage effect (`AbilityData.slk` row
    /// plus a damage handler in `effects::build_effects`).
    DataDamage { id: &'static str },
    /// Hero ability with a data-backed heal / heal-or-damage handler.
    DataHeal { id: &'static str },
    /// Hero ability with a data-backed timed buff handler.
    DataBuff { id: &'static str },
    /// Hero ability with a data-backed summon handler.
    DataSummon { id: &'static str },
    /// Active aura registered in `effects::AURAS` (engine applies the buff).
    DataAura { id: &'static str },
    /// Passive aura row: classified from the real ability sheet, no cast path.
    DataAuraPassive { id: &'static str },
    /// Transformation ultimate (metamorphosis/avatar/reincarnation rows).
    DataTransformation { id: &'static str },
    /// No engine effect yet.  Casting it logs `ABILITY_DATA_ONLY id=...` once
    /// and returns an error instead of silently paying mana for nothing.
    DataOnly { id: &'static str },
}

pub const SPELL_COUNT: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Unit,
    Point,
    None,
}

/// Data-backed ability row generated from `AbilityData.slk` (level 1).
/// Every roster ability id has one; `castable` marks the active subset that
/// `Sim::cast_data_ability` can cast through the generic path.
#[derive(Debug, Clone, Copy)]
pub struct GenericAbility {
    pub id: &'static str,
    pub name: &'static str,
    pub target: TargetKind,
    pub mana: i32,
    pub cooldown_ticks: u32,
    pub range: f32,
    pub radius: f32,
    pub power: i32,
    pub secondary: i32,
    pub duration_ticks: u32,
    pub levels: u8,
    /// Required hero level for the first rank (`reqLevel`; ultimates = 6).
    pub req_level: u8,
    pub hero: bool,
    pub castable: bool,
}

include!("abilities_gen.rs");

pub fn generic_ability(id: &str) -> Option<&'static GenericAbility> {
    GENERIC_ABILITIES.iter().find(|ability| ability.id == id)
}

/// Consumes RE's typed ability classification (read-only): one `AbilityDef`
/// per AbilityData row with `AbilityClass::Executable|DataOnly`.
pub fn typed_ability_defs(
    root: &std::path::Path,
) -> Result<Vec<wc3_re::abilities::AbilityDef>, String> {
    wc3_re::abilities::defs_for_root(root).map_err(|error| error.to_string())
}

pub fn typed_ability_summary(
    root: &std::path::Path,
) -> Result<wc3_re::abilities::AbilitySummary, String> {
    wc3_re::abilities::summary_for_root(root).map_err(|error| error.to_string())
}

/// Every data-backed ability id (active and passive).
pub fn data_ability_ids() -> impl Iterator<Item = &'static str> {
    GENERIC_ABILITIES.iter().map(|ability| ability.id)
}

/// Ability ids that the generic cast path can actually cast.
pub fn data_castable_ids() -> impl Iterator<Item = &'static str> {
    GENERIC_ABILITIES
        .iter()
        .filter(|ability| ability.castable)
        .map(|ability| ability.id)
}

#[derive(Debug, Clone, Copy)]
pub struct SpellDef {
    /// Original WC3 ability id (`AHhb`, ...).
    pub id: &'static str,
    pub name: &'static str,
    pub target: TargetKind,
    pub mana: i32,
    pub cooldown_ticks: u32,
    pub range: f32,
    pub radius: f32,
    pub power: i32,
    pub duration_ticks: u32,
    pub waves: u8,
    pub wave_interval: u32,
    pub jumps: u8,
    /// Per-jump damage decay in permille (850 = 85%).
    pub decay_permille: i32,
}

/// Numbers the game layer installs from the typed database (`AbilityData.slk`).
///
/// The hand-written [`SPELL_DEFS`] stay as the offline fallback; when the game
/// has an install it replaces every numeric field of the eleven entries with the
/// sheet values, so mana/cooldown/range/radius/duration come from the data and
/// not from this file. Target kind, wave count and jumps stay behavioural.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpellNumbers {
    pub id: &'static str,
    pub mana: i32,
    pub cooldown_ticks: u32,
    pub range: f32,
    pub radius: f32,
    pub duration_ticks: u32,
}

static INSTALLED: std::sync::OnceLock<Vec<SpellDef>> = std::sync::OnceLock::new();

/// Install sheet-derived numbers. Returns `false` when already installed (the
/// first install wins so a second `Assets` load cannot change live gameplay).
pub fn install_spell_numbers(numbers: &[SpellNumbers]) -> bool {
    if INSTALLED.get().is_some() {
        return false;
    }
    let mut table = SPELL_DEFS;
    for entry in numbers {
        if let Some(def) = table.iter_mut().find(|def| def.id == entry.id) {
            def.mana = entry.mana;
            def.cooldown_ticks = entry.cooldown_ticks;
            def.range = entry.range;
            def.radius = entry.radius;
            def.duration_ticks = entry.duration_ticks;
        }
    }
    let _ = INSTALLED.set(table.to_vec());
    true
}

/// Active spell table: installed data when present, the fallback otherwise.
pub fn spell_defs() -> &'static [SpellDef] {
    match INSTALLED.get() {
        Some(table) => table.as_slice(),
        None => &SPELL_DEFS,
    }
}

impl Spell {
    pub const ALL: [Spell; SPELL_COUNT] = [
        Self::HolyLight,
        Self::DivineShield,
        Self::Blizzard,
        Self::Heal,
        Self::EntanglingRoots,
        Self::ChainLightning,
        Self::StormBolt,
        Self::ThunderClap,
        Self::DeathCoil,
        Self::FrostNova,
        Self::CarrionSwarm,
    ];

    pub fn index(self) -> usize {
        match self {
            Self::HolyLight => 0,
            Self::DivineShield => 1,
            Self::Blizzard => 2,
            Self::Heal => 3,
            Self::EntanglingRoots => 4,
            Self::ChainLightning => 5,
            Self::StormBolt => 6,
            Self::ThunderClap => 7,
            Self::DeathCoil => 8,
            Self::FrostNova => 9,
            Self::CarrionSwarm => 10,
            // Data-mapped variants are cast through `cast_data_ability`, not
            // the typed `spell_cd` table; indexing them is a caller bug.
            Self::DataDamage { .. }
            | Self::DataHeal { .. }
            | Self::DataBuff { .. }
            | Self::DataSummon { .. }
            | Self::DataAura { .. }
            | Self::DataAuraPassive { .. }
            | Self::DataTransformation { .. }
            | Self::DataOnly { .. } => {
                panic!("non-typed ability has no SpellDef index")
            }
        }
    }

    /// WC3 ability id for typed and data-mapped variants.
    pub fn ability_id(self) -> &'static str {
        match self {
            Self::DataDamage { id }
            | Self::DataHeal { id }
            | Self::DataBuff { id }
            | Self::DataSummon { id }
            | Self::DataAura { id }
            | Self::DataAuraPassive { id }
            | Self::DataTransformation { id }
            | Self::DataOnly { id } => id,
            _ => self.def().id,
        }
    }

    /// True for the eleven hand-written spells with a `SpellDef` row.
    pub fn is_typed(self) -> bool {
        !matches!(
            self,
            Self::DataDamage { .. }
                | Self::DataHeal { .. }
                | Self::DataBuff { .. }
                | Self::DataSummon { .. }
                | Self::DataAura { .. }
                | Self::DataAuraPassive { .. }
                | Self::DataTransformation { .. }
                | Self::DataOnly { .. }
        )
    }

    /// Data-only tag when the engine has no effect for this ability.
    pub fn data_only_id(self) -> Option<&'static str> {
        match self {
            Self::DataOnly { id } => Some(id),
            _ => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|spell| spell.def().id == id)
    }

    pub fn name(self) -> &'static str {
        self.def().name
    }

    pub fn def(self) -> &'static SpellDef {
        &spell_defs()[self.index()]
    }
}

/// Transformation ultimates whose `AbilityData.slk` row is the metamorphosis
/// entry itself (Demon Hunter - Metamorphosis, Mountain King - Avatar,
/// Chieftain - Reincarnation).  The engine has no transform handler yet, so the
/// tag names the category without claiming the effect runs.
const TRANSFORMATION_IDS: &[&str] = &["AEme", "AHav", "AOre"];

static DATA_ONLY_LOGGED: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeSet<&'static str>>> =
    std::sync::OnceLock::new();

/// Logs `ABILITY_DATA_ONLY id=...` once per ability id.  Used when an ability
/// has no handler instead of spending mana on a silent no-op.
pub fn log_ability_data_only(id: &'static str) {
    let logged = DATA_ONLY_LOGGED.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeSet::new()));
    let mut set = match logged.lock() {
        Ok(set) => set,
        Err(poisoned) => poisoned.into_inner(),
    };
    if set.insert(id) {
        eprintln!("ABILITY_DATA_ONLY id={id}");
    }
}

/// True when `effects::build_effects` yields at least one handler that changes
/// game state (a positive damage/heal amount, a buff, a summon, a movement
/// modifier or an aura).
pub fn data_ability_has_effect(def: &GenericAbility) -> bool {
    use crate::effects::EffectSpec;
    crate::effects::build_effects(def).iter().any(|effect| match effect {
        EffectSpec::Damage { amount, .. }
        | EffectSpec::Heal { amount }
        | EffectSpec::HealOrDamage { amount, .. } => *amount > 0,
        EffectSpec::DamageArea { amount, .. } | EffectSpec::HealArea { amount, .. } => *amount > 0,
        EffectSpec::Buff { .. }
        | EffectSpec::Aura { .. }
        | EffectSpec::Summon { .. }
        | EffectSpec::Movement { .. } => true,
    })
}

/// Maps a hero ability id to a typed spell or a categorised data spell.
///
/// Typed spells (the eleven hand-written ones) win.  Every other id resolves
/// through its real `AbilityData.slk` row: a data effect becomes the matching
/// `Spell::DataDamage/DataHeal/DataBuff/DataSummon/DataAura/DataAuraPassive/
/// DataTransformation` variant, and ids the engine cannot execute become
/// `Spell::DataOnly` (which logs once when cast).  `None` only for ids that are
/// absent from the ability sheet.
pub fn spell_for_ability(id: &str) -> Option<Spell> {
    if let Some(typed) = Spell::from_id(id) {
        return Some(typed);
    }
    let def = generic_ability(id)?;
    let effects = crate::effects::build_effects(def);
    use crate::effects::EffectSpec;
    let has = |matcher: fn(&EffectSpec) -> bool| effects.iter().any(matcher);
    if has(|effect| matches!(effect, EffectSpec::Summon { .. })) {
        return Some(Spell::DataSummon { id: def.id });
    }
    if has(|effect| matches!(effect, EffectSpec::Buff { .. })) {
        return Some(Spell::DataBuff { id: def.id });
    }
    if effects.iter().any(|effect| match effect {
        EffectSpec::Heal { amount } | EffectSpec::HealArea { amount, .. } => *amount > 0,
        _ => false,
    }) {
        return Some(Spell::DataHeal { id: def.id });
    }
    if data_ability_has_effect(def) {
        return Some(Spell::DataDamage { id: def.id });
    }
    if crate::effects::aura_row(def.id).is_some() {
        return Some(if def.castable {
            Spell::DataAura { id: def.id }
        } else {
            Spell::DataAuraPassive { id: def.id }
        });
    }
    if TRANSFORMATION_IDS.contains(&def.id) {
        return Some(Spell::DataTransformation { id: def.id });
    }
    Some(Spell::DataOnly { id: def.id })
}

pub static SPELL_DEFS: [SpellDef; SPELL_COUNT] = [
    // UnitBalance/AbilityData observed 2026-11-12: Cool1=5 s, Cost1=75,
    // Rng1=800, Data11=200 (heal).
    SpellDef {
        id: "AHhb",
        name: "Holy Light",
        target: TargetKind::Unit,
        mana: 75,
        cooldown_ticks: 150,
        range: 800.0,
        radius: 0.0,
        power: 200,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Cool1=75 s, Cost1=125, Dur1=10 s.
    SpellDef {
        id: "AHds",
        name: "Divine Shield",
        target: TargetKind::None,
        mana: 125,
        cooldown_ticks: 2250,
        range: 0.0,
        radius: 0.0,
        power: 0,
        duration_ticks: 300,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Cool1=6 s, Cost1=75, Area1=200, Rng1=800, Data11=6 waves,
    // Data12=30 damage, Data14=0.5 s interval.
    SpellDef {
        id: "AHbz",
        name: "Blizzard",
        target: TargetKind::Point,
        mana: 75,
        cooldown_ticks: 180,
        range: 800.0,
        radius: 200.0,
        power: 30,
        duration_ticks: 0,
        waves: 6,
        wave_interval: 15,
        jumps: 0,
        decay_permille: 1000,
    },
    // Cool1=1 s, Cost1=5, Rng1=250, Data11=25.
    SpellDef {
        id: "Ahea",
        name: "Heal",
        target: TargetKind::Unit,
        mana: 5,
        cooldown_ticks: 30,
        range: 250.0,
        radius: 0.0,
        power: 25,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Cool1=8 s, Cost1=75, Rng1=800, Data11=15 damage.  Duration is shortened
    // to 6 s so one unit cannot be locked down for a third of the match on a
    // phone-sized map.
    SpellDef {
        id: "AEer",
        name: "Entangling Roots",
        target: TargetKind::Unit,
        mana: 75,
        cooldown_ticks: 240,
        range: 800.0,
        radius: 0.0,
        power: 15,
        duration_ticks: 180,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Cool1=9 s, Cost1=120, Rng1=700, Data11=100, Data12=4 jumps,
    // Data13=0.15 (15% falloff).
    SpellDef {
        id: "AOcl",
        name: "Chain Lightning",
        target: TargetKind::Unit,
        mana: 120,
        cooldown_ticks: 270,
        range: 700.0,
        radius: 500.0,
        power: 100,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 4,
        decay_permille: 850,
    },
    // Mountain King AHtb: 75 mana, 9 s, 600 range, 100 damage, 3 s stun.
    SpellDef {
        id: "AHtb",
        name: "Storm Bolt",
        target: TargetKind::Unit,
        mana: 75,
        cooldown_ticks: 270,
        range: 600.0,
        radius: 0.0,
        power: 100,
        duration_ticks: 90,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Mountain King AHtc: 90 mana, 6 s, 250 radius, 60 area damage.
    SpellDef {
        id: "AHtc",
        name: "Thunder Clap",
        target: TargetKind::None,
        mana: 90,
        cooldown_ticks: 180,
        range: 0.0,
        radius: 250.0,
        power: 60,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Death Knight AUdc: 75 mana, 5 s, 800 range, 200 heal/damage.
    SpellDef {
        id: "AUdc",
        name: "Death Coil",
        target: TargetKind::Unit,
        mana: 75,
        cooldown_ticks: 150,
        range: 800.0,
        radius: 0.0,
        power: 200,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Lich AUfn: 125 mana, 8 s, 700 range, 200 radius, 50 splash.
    SpellDef {
        id: "AUfn",
        name: "Frost Nova",
        target: TargetKind::Unit,
        mana: 125,
        cooldown_ticks: 240,
        range: 700.0,
        radius: 200.0,
        power: 50,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
    // Dread Lord AUcs: 100 mana, 8 s, 700 range, 150 radius, 75 damage.
    SpellDef {
        id: "AUcs",
        name: "Carrion Swarm",
        target: TargetKind::Point,
        mana: 100,
        cooldown_ticks: 240,
        range: 700.0,
        radius: 150.0,
        power: 75,
        duration_ticks: 0,
        waves: 0,
        wave_interval: 0,
        jumps: 0,
        decay_permille: 1000,
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpellTarget {
    None,
    Unit(UnitId),
    Point(Vec2),
}

/// A delayed area effect owned by the simulation (Blizzard waves).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingCast {
    pub due: u64,
    pub caster: UnitId,
    pub spell: Spell,
    pub at: Vec2,
    pub waves_left: u8,
    pub damage: i32,
}

impl Sim {
    /// Casts an ability for a unit.  Validation is the WC3 contract: the unit
    /// must know the spell, have mana, be off cooldown, and use the right
    /// target kind at the right range with the right allegiance.
    pub fn cast(
        &mut self,
        caster: UnitId,
        spell: Spell,
        target: SpellTarget,
    ) -> Result<(), &'static str> {
        if !spell.is_typed() {
            if let Some(id) = spell.data_only_id() {
                log_ability_data_only(id);
                return Err("ABILITY_DATA_ONLY");
            }
            let (target_unit, point) = match target {
                SpellTarget::Unit(id) => (Some(id), None),
                SpellTarget::Point(at) => (None, Some(at)),
                SpellTarget::None => (None, None),
            };
            return self.cast_data_ability(caster, spell.ability_id(), target_unit, point);
        }
        let def = spell.def();
        let Some(caster_unit) = self.unit(caster) else {
            return Err("no caster");
        };
        if !caster_unit.alive {
            return Err("caster dead");
        }
        if !caster_unit.def().spells.contains(&spell) {
            return Err("spell not available");
        }
        if caster_unit.mana < def.mana {
            return Err("not enough mana");
        }
        if caster_unit.spell_cd[spell.index()] > 0 {
            return Err("spell on cooldown");
        }
        let caster_owner = caster_unit.owner;
        let caster_pos = caster_unit.pos;
        let mut heal_target: Option<UnitId> = None;
        let mut damage_target: Option<UnitId> = None;
        let mut point: Option<Vec2> = None;

        match (def.target, target) {
            (TargetKind::None, SpellTarget::None) => {}
            (TargetKind::Unit, SpellTarget::Unit(target_id)) => {
                let Some(target_unit) = self.unit(target_id) else {
                    return Err("no target");
                };
                if !target_unit.alive {
                    return Err("target dead");
                }
                let distance = caster_pos.distance(target_unit.pos);
                if distance > def.range + target_unit.radius() {
                    return Err("out of range");
                }
                let ally = target_unit.owner == caster_owner;
                match spell {
                    Spell::HolyLight | Spell::Heal => {
                        if !ally {
                            return Err("must target an ally");
                        }
                        heal_target = Some(target_id);
                    }
                    Spell::DeathCoil => {
                        if ally {
                            heal_target = Some(target_id);
                        } else {
                            damage_target = Some(target_id);
                        }
                    }
                    Spell::EntanglingRoots
                    | Spell::ChainLightning
                    | Spell::StormBolt
                    | Spell::FrostNova => {
                        if ally {
                            return Err("must target an enemy");
                        }
                        damage_target = Some(target_id);
                    }
                    _ => return Err("wrong target kind"),
                }
            }
            (TargetKind::Point, SpellTarget::Point(at)) => {
                let distance = caster_pos.distance(at);
                if distance > def.range {
                    return Err("out of range");
                }
                point = Some(at);
            }
            _ => return Err("wrong target kind"),
        }

        if let Some(idx) = self.unit_index(caster) {
            let unit = &mut self.units[idx];
            unit.mana -= def.mana;
            unit.spell_cd[spell.index()] = def.cooldown_ticks;
        }

        match spell {
            Spell::HolyLight | Spell::Heal => {
                if let Some(target_id) = heal_target {
                    let healed = self.heal_unit(target_id, def.power);
                    self.events.push(Event::SpellHeal {
                        caster,
                        spell,
                        to: target_id,
                        amount: healed,
                    });
                }
            }
            Spell::DivineShield => {
                if let Some(idx) = self.unit_index(caster) {
                    self.units[idx].invulnerable_until = self.tick + def.duration_ticks as u64;
                }
            }
            Spell::EntanglingRoots => {
                if let Some(target_id) = damage_target {
                    if let Some(idx) = self.unit_index(target_id) {
                        self.units[idx].rooted_until = self.tick + def.duration_ticks as u64;
                        self.units[idx].path.clear();
                        self.units[idx].path_index = 0;
                    }
                    let dealt = self.apply_damage(caster, target_id, def.power, AttackType::Magic);
                    self.events.push(Event::SpellHit {
                        caster,
                        spell,
                        to: target_id,
                        amount: dealt,
                    });
                }
            }
            Spell::ChainLightning => {
                if let Some(first) = damage_target {
                    let mut current = first;
                    let mut damage = def.power;
                    let mut hit = vec![current];
                    for _ in 0..=def.jumps {
                        let dealt = self.apply_damage(caster, current, damage, AttackType::Magic);
                        self.events.push(Event::SpellHit {
                            caster,
                            spell,
                            to: current,
                            amount: dealt,
                        });
                        damage = damage * def.decay_permille / 1000;
                        let Some(next) = self.nearest_enemy_near(
                            caster_owner,
                            self.pos_of(current),
                            def.radius,
                            &hit,
                        ) else {
                            break;
                        };
                        hit.push(next);
                        current = next;
                    }
                }
            }
            Spell::StormBolt => {
                if let Some(target_id) = damage_target {
                    let dealt = self.apply_damage(caster, target_id, def.power, AttackType::Magic);
                    if let Some(index) = self.unit_index(target_id) {
                        self.units[index].stunned_until = self.tick + def.duration_ticks as u64;
                        self.units[index].path.clear();
                    }
                    self.events.push(Event::SpellHit {
                        caster,
                        spell,
                        to: target_id,
                        amount: dealt,
                    });
                }
            }
            Spell::ThunderClap => {
                let center = self.pos_of(caster);
                self.damage_area(caster, center, def.radius, def.power, spell);
            }
            Spell::DeathCoil => {
                if let Some(target_id) = heal_target {
                    let healed = self.heal_unit(target_id, def.power);
                    self.events.push(Event::SpellHeal {
                        caster,
                        spell,
                        to: target_id,
                        amount: healed,
                    });
                } else if let Some(target_id) = damage_target {
                    let dealt = self.apply_damage(caster, target_id, def.power, AttackType::Magic);
                    self.events.push(Event::SpellHit {
                        caster,
                        spell,
                        to: target_id,
                        amount: dealt,
                    });
                }
            }
            Spell::FrostNova => {
                if let Some(target_id) = damage_target {
                    let center = self.pos_of(target_id);
                    let dealt = self.apply_damage(caster, target_id, def.power, AttackType::Magic);
                    self.events.push(Event::SpellHit {
                        caster,
                        spell,
                        to: target_id,
                        amount: dealt,
                    });
                    self.damage_area(caster, center, def.radius, def.power / 2, spell);
                }
            }
            Spell::CarrionSwarm => {
                if let Some(at) = point {
                    self.damage_area(caster, at, def.radius, def.power, spell);
                }
            }
            Spell::Blizzard => {
                if let Some(at) = point {
                    if let Some(index) = self.unit_index(caster) {
                        self.units[index].channel_until =
                            self.tick + (def.wave_interval as u64) * def.waves.max(1) as u64;
                    }
                    self.pending_casts.push(PendingCast {
                        due: self.tick + def.wave_interval as u64,
                        caster,
                        spell,
                        at,
                        waves_left: def.waves,
                        damage: def.power,
                    });
                }
            }
            // Data-mapped variants are dispatched to `cast_data_ability` above.
            _ => {}
        }

        // Chain Lightning can kill the caster's target; the event list already
        // contains the damage events from `apply_damage`.
        self.events.push(Event::SpellCast {
            caster,
            spell,
            target,
        });
        Ok(())
    }

    /// Resolves delayed spell waves.  Called once per tick before movement.
    pub(crate) fn advance_pending_casts(&mut self) {
        if self.pending_casts.is_empty() {
            return;
        }
        let tick = self.tick;
        let mut index = 0;
        while index < self.pending_casts.len() {
            let due = self.pending_casts[index].due;
            if due > tick {
                index += 1;
                continue;
            }
            let cast = self.pending_casts[index];
            if let Some(caster) = self.unit(cast.caster) {
                if caster.alive {
                    let owner = caster.owner;
                    let interval = cast.spell.def().wave_interval as u64;
                    let radius = cast.spell.def().radius;
                    let mut indices = std::mem::take(&mut self.spatial_buffer);
                    self.spatial.query(cast.at, radius, 64, &mut indices);
                    let mut targets: Vec<(UnitId, f32)> = Vec::new();
                    for &raw in &indices {
                        let Some(unit) = self.units.get(raw as usize) else {
                            continue;
                        };
                        if !unit.alive || unit.owner == owner {
                            continue;
                        }
                        let distance = unit.pos.distance(cast.at);
                        if distance <= radius + unit.radius() {
                            targets.push((unit.id, distance));
                        }
                    }
                    self.spatial_buffer = indices;
                    targets.sort_by(|a, b| {
                        a.1.partial_cmp(&b.1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(a.0.cmp(&b.0))
                    });
                    targets.dedup_by_key(|(target, _)| *target);
                    for (target, _) in targets {
                        let dealt =
                            self.apply_damage(cast.caster, target, cast.damage, AttackType::Magic);
                        if dealt > 0 {
                            self.events.push(Event::SpellHit {
                                caster: cast.caster,
                                spell: cast.spell,
                                to: target,
                                amount: dealt,
                            });
                        }
                    }
                    if cast.waves_left > 1 {
                        let pending = &mut self.pending_casts[index];
                        pending.waves_left -= 1;
                        pending.due = tick + interval;
                        index += 1;
                        continue;
                    }
                }
            }
            self.pending_casts.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::def_of;

    #[test]
    fn spell_parameters_match_abilitydata() {
        assert_eq!(Spell::HolyLight.def().mana, 75);
        assert_eq!(Spell::HolyLight.def().cooldown_ticks, 150);
        assert_eq!(Spell::HolyLight.def().power, 200);
        assert_eq!(Spell::Blizzard.def().waves, 6);
        assert_eq!(Spell::Blizzard.def().radius, 200.0);
        assert_eq!(Spell::ChainLightning.def().power, 100);
        assert_eq!(Spell::ChainLightning.def().jumps, 4);
        assert_eq!(Spell::DivineShield.def().duration_ticks, 300);
    }

    #[test]
    fn roster_casters_know_their_spells() {
        let paladin = def_of("Hpal").unwrap();
        assert!(paladin.spells.contains(&Spell::HolyLight));
        assert!(paladin.spells.contains(&Spell::DivineShield));
        let archmage = def_of("Hamg").unwrap();
        assert!(archmage.spells.contains(&Spell::Blizzard));
        let keeper = def_of("Ekee").unwrap();
        assert!(keeper.spells.contains(&Spell::EntanglingRoots));
        let far_seer = def_of("Ofar").unwrap();
        assert!(far_seer.spells.contains(&Spell::ChainLightning));
    }

    #[test]
    fn typed_damage_reference() {
        assert_eq!(
            crate::data::typed_damage(AttackType::Magic, crate::data::ArmorType::Heavy, 0, 30),
            60
        );
    }
}

impl Sim {
    /// Generic data-driven cast path for every active ability in
    /// `ability_ids` (mana/cooldown/range/target from `AbilityData.slk`).
    ///
    /// Effect policy for abilities without a hand-written `Spell`: a unit
    /// target is healed when allied and damaged when hostile using `DataA1` as
    /// the power; a point target damages enemies inside `Area1`; self/no-target
    /// abilities pay their real cost and emit the cast event.
    pub fn cast_data_ability(
        &mut self,
        caster: UnitId,
        ability_id: &str,
        target: Option<UnitId>,
        point: Option<Vec2>,
    ) -> Result<(), &'static str> {
        self.cast_data_ability_from(caster, ability_id, target, point, false)
    }

    /// Casts an ability whose source is the carried item, not the unit card.
    ///
    /// `pdiv` (Potion of Divinity) grants `AIdv` to any carrier, so the unit's
    /// static `abilList` and the `Rhde`-style research gate do not apply: the
    /// item is the source.  Item casts still pay the ability's real mana,
    /// cooldown and target rules.
    pub fn cast_item_ability(
        &mut self,
        caster: UnitId,
        ability_id: &str,
        target: Option<UnitId>,
        point: Option<Vec2>,
    ) -> Result<(), &'static str> {
        self.cast_data_ability_from(caster, ability_id, target, point, true)
    }

    fn cast_data_ability_from(
        &mut self,
        caster: UnitId,
        ability_id: &str,
        target: Option<UnitId>,
        point: Option<Vec2>,
        from_item: bool,
    ) -> Result<(), &'static str> {
        let def = generic_ability(ability_id).ok_or("ability not in data")?;
        if !def.castable {
            return Err("ability is passive");
        }
        let Some(caster_unit) = self.unit(caster) else {
            return Err("no caster");
        };
        if !caster_unit.alive {
            return Err("caster dead");
        }
        // Ability-granting upgrades (Defend, Control Magic, ...) stay locked
        // until the unit's `Upgrades` research is complete for the owner.
        if !from_item && !self.unit_ability_unlocked(caster, def.id) {
            return Err("ability not researched");
        }
        if caster_unit.def().hero
            && caster_unit.def().ability_ids.contains(&def.id)
            && !caster_unit.learned.contains(&def.id)
        {
            return Err("ability not learned");
        }
        if caster_unit.mana < def.mana {
            return Err("not enough mana");
        }
        if caster_unit
            .data_cd
            .iter()
            .any(|(id, cooldown)| *id == def.id && *cooldown > 0)
        {
            return Err("ability on cooldown");
        }
        let caster_pos = caster_unit.pos;
        match def.target {
            TargetKind::Unit => {
                let target_id = target.ok_or("no target")?;
                let target_unit = self.unit(target_id).ok_or("no target")?;
                if !target_unit.alive {
                    return Err("target dead");
                }
                if caster_pos.distance(target_unit.pos) > def.range.max(1.0) + target_unit.radius()
                {
                    return Err("out of range");
                }
            }
            TargetKind::Point => {
                let at = point.ok_or("no point")?;
                if caster_pos.distance(at) > def.range.max(1.0) {
                    return Err("out of range");
                }
            }
            TargetKind::None => {}
        }
        // An ability the handler table cannot execute must not pay mana and
        // emit a cast event for nothing: tag it and let the caller see why.
        let effects = crate::effects::build_effects(def);
        if !data_ability_has_effect(def) {
            log_ability_data_only(def.id);
            return Err("ABILITY_DATA_ONLY");
        }
        if let Some(index) = self.unit_index(caster) {
            let unit = &mut self.units[index];
            unit.mana -= def.mana;
            unit.data_cd.retain(|(id, _)| *id != def.id);
            if def.cooldown_ticks > 0 {
                unit.data_cd.push((def.id, def.cooldown_ticks));
            }
        }
        self.events.push(Event::DataAbilityCast {
            caster,
            ability: def.id,
            target,
        });
        // Handler path: every ability the effect table can express runs through
        // the generic runner, so buffs/auras/summons/movement share one code
        // path with damage and healing.
        let ctx = crate::effects::EffectContext {
            caster,
            target,
            point,
        };
        let outcomes = crate::effects::run_effects(self, ctx, &effects);
        crate::effects::emit_events(self, caster, def.id, &outcomes);
        Ok(())
    }
}
