//! Typed WC3 ability table: every `AbilityData.slk` row is classified as
//! **executable** (an effect kind the generic runner can apply) or
//! **data-only** with a machine-readable reason.
//!
//! The 763 shipped rows are parsed with the canonical `wc3-re::slk::Sheet`
//! reader. There is no "unknown" state: [`AbilityClass`] is exhaustive, so a
//! row without a runnable effect always carries a [`DataOnlyReason`].
//!
//! Buff/effect tables are optional: the current TFT install ships neither
//! `BuffData.slk` nor `EffectData.slk`, so `buff` is `None` and the parser
//! reads `Buff1`/`Effect1` when a future install provides those columns. The
//! source of the buff field is part of the artifact (`buff_source`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{ReError, Result};
use crate::manifest::json_str;
use crate::slk::Sheet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    Unit,
    Point,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectKind {
    Damage,
    Heal,
    Buff,
    Summon,
    Aura,
    Movement,
}

impl EffectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Damage => "damage",
            Self::Heal => "heal",
            Self::Buff => "buff",
            Self::Summon => "summon",
            Self::Aura => "aura",
            Self::Movement => "movement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataOnlyReason {
    NoLevels,
    ItemAbility,
    Passive,
    NoManaCost,
    NoEffectPayload,
    MissingEffectParams,
    NoTarget,
    UnsupportedSignature,
}

impl DataOnlyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoLevels => "no_levels",
            Self::ItemAbility => "item_ability",
            Self::Passive => "passive",
            Self::NoManaCost => "no_mana_cost",
            Self::NoEffectPayload => "no_effect_payload",
            Self::MissingEffectParams => "missing_effect_params",
            Self::NoTarget => "no_target",
            Self::UnsupportedSignature => "unsupported_signature",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityClass {
    Executable(EffectKind),
    DataOnly(DataOnlyReason),
}

impl AbilityClass {
    pub fn executable_kind(self) -> Option<EffectKind> {
        match self {
            Self::Executable(kind) => Some(kind),
            Self::DataOnly(_) => None,
        }
    }

    pub fn reason(self) -> Option<DataOnlyReason> {
        match self {
            Self::Executable(_) => None,
            Self::DataOnly(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectParams {
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub c: Option<f64>,
    pub d: Option<f64>,
    pub e: Option<f64>,
    pub unit_id: Option<String>,
    pub buff: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AbilityDef {
    pub id: String,
    pub name: String,
    pub levels: i32,
    pub target: TargetKind,
    pub mana: Option<f64>,
    pub cooldown: Option<f64>,
    pub range: Option<f64>,
    pub area: Option<f64>,
    pub duration: Option<f64>,
    pub hero: bool,
    pub item: bool,
    pub hostile: bool,
    pub class: AbilityClass,
    pub effect_params: EffectParams,
    pub buff: Option<String>,
    pub buff_source: String,
}

impl AbilityDef {
    pub fn is_executable(&self) -> bool {
        matches!(self.class, AbilityClass::Executable(_))
    }

    pub fn effect_kind(&self) -> Option<EffectKind> {
        self.class.executable_kind()
    }

    pub fn data_only_reason(&self) -> Option<DataOnlyReason> {
        self.class.reason()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AbilitySummary {
    pub total: usize,
    pub executable: usize,
    pub data_only: usize,
    pub by_effect: BTreeMap<&'static str, usize>,
    pub by_reason: BTreeMap<&'static str, usize>,
}

impl AbilitySummary {
    pub fn from_defs(defs: &[AbilityDef]) -> Self {
        let mut summary = Self {
            total: defs.len(),
            ..Self::default()
        };
        for def in defs {
            match def.class {
                AbilityClass::Executable(kind) => {
                    summary.executable += 1;
                    *summary.by_effect.entry(kind.as_str()).or_insert(0) += 1;
                }
                AbilityClass::DataOnly(reason) => {
                    summary.data_only += 1;
                    *summary.by_reason.entry(reason.as_str()).or_insert(0) += 1;
                }
            }
        }
        summary
    }
}

fn ability_path(root: &Path) -> Option<PathBuf> {
    [
        root.join("analysis/war3x_data/Units/AbilityData.slk"),
        root.join("analysis/war3_full3/Units/AbilityData.slk"),
        root.join("Units/AbilityData.slk"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn number(sheet: &Sheet, row: usize, column: &str) -> Option<f64> {
    let value = sheet.cell(row, column)?.trim();
    if value.is_empty() || value == "-" || value == " - " {
        return None;
    }
    value.parse().ok()
}

fn integer(sheet: &Sheet, row: usize, column: &str) -> Option<i32> {
    number(sheet, row, column).map(|value| value as i32)
}

fn text(sheet: &Sheet, row: usize, column: &str) -> Option<String> {
    let value = sheet.cell(row, column)?.trim();
    if value.is_empty() || value == "-" || value == " - " {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_target(targs: &str, area: Option<f64>) -> TargetKind {
    let lower = targs.to_ascii_lowercase();
    if lower.contains("friend") || lower.contains("self") {
        return TargetKind::Unit;
    }
    let hostile = ["enemy", "neutral", "organic", "ground", "air", "tree", "debris", "alive"]
        .iter()
        .any(|keyword| lower.contains(keyword));
    if !hostile {
        return TargetKind::None;
    }
    if lower.contains("tree") || lower.contains("debris") {
        return TargetKind::Point;
    }
    if area.is_some_and(|value| value > 0.0) {
        TargetKind::Point
    } else {
        TargetKind::Unit
    }
}

fn is_hostile(targs: &str) -> bool {
    let lower = targs.to_ascii_lowercase();
    !lower.contains("friend")
        && !lower.contains("self")
        && ["enemy", "neutral", "organic", "ground", "air", "tree", "debris"]
            .iter()
            .any(|keyword| lower.contains(keyword))
}

fn keyword_kind(text: &str) -> Option<EffectKind> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("aura") {
        return Some(EffectKind::Aura);
    }
    if lower.contains("summon") {
        return Some(EffectKind::Summon);
    }
    if ["heal", "holy light", "rejuvenation", "tranquility", "healing"].iter().any(|word| lower.contains(word)) {
        return Some(EffectKind::Heal);
    }
    if ["slow", "haste", "speed", "teleport", "blink", "wind walk", "charge"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Some(EffectKind::Movement);
    }
    if ["damage", "bolt", "nova", "blizzard", "shock", "strike", "wave", "lightning", "breath", "cleave", "coil", "clap", "swarm"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Some(EffectKind::Damage);
    }
    if ["armor", "shield", "curse", "poison", "stun", "root", "ensnare", "web", "buff", "frost armor"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Some(EffectKind::Buff);
    }
    None
}

fn known_kind(id: &str) -> Option<EffectKind> {
    match id {
        "AHhb" | "Ahea" => Some(EffectKind::Heal),
        "AHds" => Some(EffectKind::Buff),
        "AHbz" | "AOcl" | "AHtb" | "AHtc" | "AUdc" | "AUfn" | "AUcs" => {
            Some(EffectKind::Damage)
        }
        "AEer" => Some(EffectKind::Buff),
        _ => None,
    }
}

fn classify(
    id: &str,
    name: &str,
    targs: &str,
    levels: i32,
    item: bool,
    target: TargetKind,
    mana: Option<f64>,
    cooldown: Option<f64>,
    area: Option<f64>,
    duration: Option<f64>,
    params: &EffectParams,
) -> AbilityClass {
    if levels <= 0 {
        return AbilityClass::DataOnly(DataOnlyReason::NoLevels);
    }
    if item {
        return AbilityClass::DataOnly(DataOnlyReason::ItemAbility);
    }
    if params.unit_id.is_some() {
        return AbilityClass::Executable(EffectKind::Summon);
    }
    let kind = known_kind(id)
        .or_else(|| keyword_kind(name))
        .or_else(|| {
            let has_payload = params.a.is_some();
            if has_payload {
                if target == TargetKind::Unit && !is_hostile(targs) {
                    Some(EffectKind::Heal)
                } else if area.unwrap_or(0.0) > 0.0 || is_hostile(targs) {
                    Some(EffectKind::Damage)
                } else if duration.unwrap_or(0.0) > 0.0 {
                    Some(EffectKind::Buff)
                } else {
                    Some(EffectKind::Damage)
                }
            } else if area.unwrap_or(0.0) > 0.0 && duration.unwrap_or(0.0) > 0.0 {
                Some(EffectKind::Aura)
            } else {
                None
            }
        });
    let passive = mana.unwrap_or(0.0) == 0.0
        && cooldown.unwrap_or(0.0) == 0.0
        && targs.trim() == "_"
        && area.unwrap_or(0.0) == 0.0
        && duration.unwrap_or(0.0) == 0.0
        && params.a.is_none()
        && params.unit_id.is_none();
    if passive && kind.is_none() {
        return AbilityClass::DataOnly(DataOnlyReason::Passive);
    }
    let Some(kind) = kind else {
        return AbilityClass::DataOnly(if target == TargetKind::None && area.unwrap_or(0.0) == 0.0 {
            DataOnlyReason::NoTarget
        } else {
            DataOnlyReason::NoEffectPayload
        });
    };
    let valid = match kind {
        EffectKind::Damage | EffectKind::Heal => params.a.is_some_and(|value| value > 0.0),
        EffectKind::Summon => params.unit_id.is_some(),
        EffectKind::Aura => area.unwrap_or(0.0) > 0.0,
        EffectKind::Movement => params.a.is_some() && duration.unwrap_or(0.0) > 0.0,
        EffectKind::Buff => duration.unwrap_or(0.0) > 0.0 || area.unwrap_or(0.0) > 0.0,
    };
    if valid {
        AbilityClass::Executable(kind)
    } else {
        AbilityClass::DataOnly(DataOnlyReason::MissingEffectParams)
    }
}

fn parse_def(sheet: &Sheet, row: usize) -> Option<AbilityDef> {
    let id = sheet.rows.get(row)?.first()?.trim().to_string();
    if id.is_empty() {
        return None;
    }
    let name = text(sheet, row, "comments")
        .or_else(|| text(sheet, row, "code"))
        .unwrap_or_else(|| id.clone());
    let levels = integer(sheet, row, "levels").unwrap_or(0);
    let targs = text(sheet, row, "targs").unwrap_or_default();
    let mana = number(sheet, row, "Cost1");
    let cooldown = number(sheet, row, "Cool1");
    let range = number(sheet, row, "Rng1");
    let area = number(sheet, row, "Area1").filter(|value| *value > 0.0);
    let duration = number(sheet, row, "Dur1").or_else(|| number(sheet, row, "HeroDur1"));
    let hero = integer(sheet, row, "hero").unwrap_or(0) != 0;
    let item = integer(sheet, row, "item").unwrap_or(0) != 0;
    let params = EffectParams {
        a: number(sheet, row, "DataA1"),
        b: number(sheet, row, "DataB1"),
        c: number(sheet, row, "DataC1"),
        d: number(sheet, row, "DataD1"),
        e: number(sheet, row, "DataE1"),
        unit_id: text(sheet, row, "UnitID1"),
        buff: text(sheet, row, "Buff1"),
    };
    let buff_source = if sheet.column_index("Buff1").is_some() {
        "AbilityData.Buff1"
    } else {
        "buff_field_absent_in_install"
    }
    .to_string();
    let target = parse_target(&targs, area);
    let class = classify(
        &id, &name, &targs, levels, item, target, mana, cooldown, area, duration, &params,
    );
    let mut hostile = is_hostile(&targs);
    match class {
        AbilityClass::Executable(EffectKind::Damage) => hostile = true,
        AbilityClass::Executable(EffectKind::Heal | EffectKind::Aura) => hostile = false,
        _ => {}
    }
    Some(AbilityDef {
        id,
        name,
        levels,
        target,
        mana,
        cooldown,
        range,
        area,
        duration,
        hero,
        item,
        hostile,
        class,
        buff: params.buff.clone(),
        buff_source,
        effect_params: params,
    })
}

/// Parse and classify every row of `AbilityData.slk`.
pub fn defs_for_root(root: &Path) -> Result<Vec<AbilityDef>> {
    let path = ability_path(root)
        .ok_or_else(|| ReError::NotFound("AbilityData.slk (analysis/war3x_data/Units)".into()))?;
    let bytes = fs::read(&path)?;
    let sheet = Sheet::parse("AbilityData", &bytes)?;
    let mut defs = Vec::with_capacity(sheet.rows.len());
    for row in 0..sheet.rows.len() {
        if let Some(def) = parse_def(&sheet, row) {
            defs.push(def);
        }
    }
    if defs.is_empty() {
        return Err(ReError::Parse(format!(
            "AbilityData.slk has no rows: {}",
            path.display()
        )));
    }
    Ok(defs)
}

/// Parse and classify the shipped TFT table (repo root inferred).
pub fn defs() -> Result<Vec<AbilityDef>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    defs_for_root(&root)
}

pub fn summary_for_root(root: &Path) -> Result<AbilitySummary> {
    let defs = defs_for_root(root)?;
    Ok(AbilitySummary::from_defs(&defs))
}

pub fn summary() -> Result<AbilitySummary> {
    let defs = defs()?;
    Ok(AbilitySummary::from_defs(&defs))
}

impl AbilityDef {
    pub fn to_json(&self) -> String {
        let params = &self.effect_params;
        format!(
            "{{\"id\":{},\"name\":{},\"levels\":{},\"target\":\"{}\",\"mana\":{},\"cooldown\":{},\"range\":{},\"area\":{},\"duration\":{},\"hero\":{},\"item\":{},\"hostile\":{},\"class\":\"{}\",\"effect\":{},\"reason\":{},\"params\":{{\"a\":{},\"b\":{},\"c\":{},\"d\":{},\"e\":{},\"unit_id\":{}}},\"buff\":{},\"buff_source\":{}}}",
            json_str(&self.id),
            json_str(&self.name),
            self.levels,
            match self.target {
                TargetKind::Unit => "unit",
                TargetKind::Point => "point",
                TargetKind::None => "none",
            },
            opt_num(self.mana),
            opt_num(self.cooldown),
            opt_num(self.range),
            opt_num(self.area),
            opt_num(self.duration),
            self.hero,
            self.item,
            self.hostile,
            match self.class {
                AbilityClass::Executable(_) => "executable",
                AbilityClass::DataOnly(_) => "data_only",
            },
            self.effect_kind()
                .map_or_else(|| "null".to_string(), |kind| json_str(kind.as_str())),
            self.data_only_reason()
                .map_or_else(|| "null".to_string(), |reason| json_str(reason.as_str())),
            opt_num(params.a),
            opt_num(params.b),
            opt_num(params.c),
            opt_num(params.d),
            opt_num(params.e),
            params
                .unit_id
                .as_deref()
                .map_or_else(|| "null".to_string(), json_str),
            self.buff.as_deref().map_or_else(|| "null".to_string(), json_str),
            json_str(&self.buff_source)
        )
    }
}

fn opt_num(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_string(), |value| format!("{value}"))
}

impl AbilitySummary {
    pub fn to_json(&self) -> String {
        let _ = self;
        format!(
            "{{\"total\":{},\"executable\":{},\"data_only\":{},\"by_effect\":{},\"by_reason\":{}}}",
            self.total,
            self.executable,
            self.data_only,
            json_counts(&self.by_effect),
            json_counts(&self.by_reason)
        )
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# ABILITY CLASSIFICATION");
        let _ = writeln!(
            out,
            "total **{}** · executable **{}** · data-only **{}**",
            self.total, self.executable, self.data_only
        );
        let _ = writeln!(out, "\n## Executable effects");
        for (kind, count) in &self.by_effect {
            let _ = writeln!(out, "- `{kind}`: {count}");
        }
        let _ = writeln!(out, "\n## Data-only reasons");
        for (reason, count) in &self.by_reason {
            let _ = writeln!(out, "- `{reason}`: {count}");
        }
        out
    }
}

fn json_counts(map: &BTreeMap<&'static str, usize>) -> String {
    map.iter()
        .map(|(key, count)| format!("{}:{}", json_str(key), count))
        .collect::<Vec<_>>()
        .join(",")
        .pipe_braces()
}

trait PipeBraces {
    fn pipe_braces(self) -> String;
}

impl PipeBraces for String {
    fn pipe_braces(self) -> String {
        format!("{{{self}}}")
    }
}

// ---------------------------------------------------------------- effect runner

#[derive(Debug, Clone, PartialEq)]
pub enum EffectTarget {
    Unit(u32),
    Point(f64, f64),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveBuff {
    pub ability: String,
    pub kind: EffectKind,
    pub magnitude: f64,
    pub remaining: f64,
}

#[derive(Debug, Clone)]
pub struct EffectUnit {
    pub owner: u32,
    pub pos: (f64, f64),
    pub hp: f64,
    pub max_hp: f64,
    pub speed: f64,
    pub alive: bool,
    pub buffs: Vec<ActiveBuff>,
}

impl EffectUnit {
    pub fn new(owner: u32, pos: (f64, f64), hp: f64) -> Self {
        Self {
            owner,
            pos,
            hp,
            max_hp: hp,
            speed: 270.0,
            alive: true,
            buffs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EffectWorld {
    pub units: BTreeMap<u32, EffectUnit>,
    pub next_summon: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectOutcome {
    pub kind: Option<EffectKind>,
    pub affected: usize,
    pub damage: f64,
    pub healing: f64,
    pub buffs_applied: usize,
    pub summons: usize,
}

fn within(unit: &EffectUnit, point: (f64, f64), radius: f64) -> bool {
    let dx = unit.pos.0 - point.0;
    let dy = unit.pos.1 - point.1;
    dx * dx + dy * dy <= radius * radius
}

fn target_units(
    def: &AbilityDef,
    caster: u32,
    target: &EffectTarget,
    world: &EffectWorld,
) -> Vec<u32> {
    let radius = def.area.unwrap_or(0.0).max(0.0);
    match target {
        EffectTarget::Unit(unit) => world.units.contains_key(unit).then_some(*unit).into_iter().collect(),
        EffectTarget::Point(x, y) if radius > 0.0 => world
            .units
            .iter()
            .filter(|(_, unit)| unit.alive && within(unit, (*x, *y), radius))
            .filter(|(_, unit)| {
                let same_owner = world
                    .units
                    .get(&caster)
                    .is_some_and(|source| source.owner == unit.owner);
                if def.hostile {
                    !same_owner
                } else {
                    same_owner
                }
            })
            .map(|(handle, _)| *handle)
            .collect(),
        EffectTarget::Point(x, y) => world
            .units
            .iter()
            .filter(|(_, unit)| unit.alive && unit.pos == (*x, *y))
            .map(|(handle, _)| *handle)
            .collect(),
        EffectTarget::None => {
            if def.target == TargetKind::None {
                vec![caster]
            } else {
                Vec::new()
            }
        }
    }
}

/// Apply one classified executable ability to a minimal effect world.
///
/// The simulation layer owns full unit/AI semantics; this API is the typed,
/// testable effect resolution that the sim/game bridge can adopt without a
/// second ability parser. Damage/heal/buff/aura/movement/summon are supported;
/// data-only abilities return an error naming their classification.
pub fn apply_effect(
    def: &AbilityDef,
    caster: u32,
    target: EffectTarget,
    world: &mut EffectWorld,
) -> Result<EffectOutcome> {
    let Some(kind) = def.effect_kind() else {
        return Err(ReError::Unsupported(format!(
            "ability {} is data-only ({})",
            def.id,
            def.data_only_reason()
                .map_or("unknown", DataOnlyReason::as_str)
        )));
    };
    let mut outcome = EffectOutcome {
        kind: Some(kind),
        ..EffectOutcome::default()
    };
    let magnitude = def.effect_params.a.unwrap_or(0.0);
    let duration = def.duration.unwrap_or(0.0);
    if kind == EffectKind::Summon {
        let unit_id = def
            .effect_params
            .unit_id
            .as_deref()
            .ok_or_else(|| ReError::Unsupported("summon without UnitID1".into()))?;
        let count = magnitude.max(1.0).min(8.0) as usize;
        let pos = world
            .units
            .get(&caster)
            .map(|unit| unit.pos)
            .unwrap_or((0.0, 0.0));
        let owner = world.units.get(&caster).map_or(0, |unit| unit.owner);
        for _ in 0..count {
            let handle = 0x8000_0000u32.saturating_add(world.next_summon);
            world.next_summon += 1;
            world
                .units
                .insert(handle, EffectUnit::new(owner, pos, 100.0));
            outcome.summons += 1;
        }
        let _ = unit_id;
        return Ok(outcome);
    }
    let targets = target_units(def, caster, &target, world);
    for handle in targets {
        let Some(unit) = world.units.get_mut(&handle) else {
            continue;
        };
        if !unit.alive {
            continue;
        }
        match kind {
            EffectKind::Damage => {
                unit.hp -= magnitude;
                outcome.damage += magnitude;
                if unit.hp <= 0.0 {
                    unit.hp = 0.0;
                    unit.alive = false;
                }
            }
            EffectKind::Heal => {
                let before = unit.hp;
                unit.hp = (unit.hp + magnitude).min(unit.max_hp);
                outcome.healing += unit.hp - before;
            }
            EffectKind::Buff | EffectKind::Aura => {
                unit.buffs.push(ActiveBuff {
                    ability: def.id.clone(),
                    kind,
                    magnitude,
                    remaining: duration,
                });
                outcome.buffs_applied += 1;
            }
            EffectKind::Movement => {
                unit.speed += magnitude;
                unit.buffs.push(ActiveBuff {
                    ability: def.id.clone(),
                    kind,
                    magnitude,
                    remaining: duration,
                });
                outcome.buffs_applied += 1;
            }
            EffectKind::Summon => unreachable!(),
        }
        outcome.affected += 1;
    }
    Ok(outcome)
}

impl AbilityDef {
    pub fn executable_to_json(&self) -> String {
        self.to_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn synthetic(kind: EffectKind, a: f64, duration: f64, hostile: bool) -> AbilityDef {
        AbilityDef {
            id: "TEST".into(),
            name: "Test".into(),
            levels: 1,
            target: TargetKind::Unit,
            mana: Some(10.0),
            cooldown: Some(1.0),
            range: Some(500.0),
            area: None,
            duration: Some(duration),
            hero: false,
            item: false,
            hostile,
            class: AbilityClass::Executable(kind),
            effect_params: EffectParams {
                a: Some(a),
                ..EffectParams::default()
            },
            buff: None,
            buff_source: "test".into(),
        }
    }

    #[test]
    fn abilities_classification_covers_763_rows() {
        let root = root();
        assert!(
            ability_path(&root).is_some(),
            "AbilityData.slk missing under {}",
            root.display()
        );
        let defs = defs_for_root(&root).expect("defs");
        assert_eq!(defs.len(), 763, "AbilityData rows");
        let summary = AbilitySummary::from_defs(&defs);
        assert_eq!(summary.total, 763);
        assert_eq!(summary.executable + summary.data_only, 763);
        assert_eq!(summary.total - summary.executable, summary.data_only);
        for def in &defs {
            match def.class {
                AbilityClass::Executable(kind) => assert_eq!(def.effect_kind(), Some(kind)),
                AbilityClass::DataOnly(reason) => {
                    assert_eq!(def.data_only_reason(), Some(reason));
                    assert!(!reason.as_str().is_empty());
                }
            }
        }
        for (id, kind) in [
            ("AHbz", EffectKind::Damage),
            ("AHhb", EffectKind::Heal),
            ("AHds", EffectKind::Buff),
            ("Ahea", EffectKind::Heal),
            ("AOcl", EffectKind::Damage),
            ("AUdc", EffectKind::Damage),
            ("AUfn", EffectKind::Damage),
            ("AUcs", EffectKind::Damage),
        ] {
            let def = defs
                .iter()
                .find(|def| def.id == id)
                .unwrap_or_else(|| panic!("{id} missing"));
            assert_eq!(def.effect_kind(), Some(kind), "{id}: {:?}", def.class);
        }
        assert!(summary.executable > 0, "executable {}", summary.executable);
        assert!(summary.data_only > 0, "data-only {}", summary.data_only);
        assert!(defs
            .iter()
            .filter(|def| !def.is_executable())
            .all(|def| def.data_only_reason().is_some()));
        let json = summary.to_json();
        assert!(json.contains("\"total\":763"), "{json}");
    }

    #[test]
    fn abilities_execution_runs_classified_effects() {
        let root = root();
        assert!(ability_path(&root).is_some(), "AbilityData.slk missing");
        let defs = defs_for_root(&root).expect("defs");
        let mut world = EffectWorld::default();
        world.units.insert(1, EffectUnit::new(0, (0.0, 0.0), 500.0));
        world.units.insert(2, EffectUnit::new(0, (50.0, 0.0), 500.0));
        world.units.insert(3, EffectUnit::new(1, (100.0, 0.0), 500.0));

        let damage = defs
            .iter()
            .find(|def| def.id == "AHbz")
            .expect("AHbz");
        let outcome = apply_effect(damage, 1, EffectTarget::Point(100.0, 0.0), &mut world)
            .expect("damage");
        assert!(outcome.damage > 0.0, "damage {outcome:?}");
        assert!(world.units[&3].hp < 500.0);

        let heal = defs
            .iter()
            .find(|def| def.id == "AHhb")
            .expect("AHhb");
        world.units.get_mut(&2).expect("ally").hp = 300.0;
        let before = world.units[&2].hp;
        let outcome = apply_effect(heal, 1, EffectTarget::Unit(2), &mut world).expect("heal");
        assert!(outcome.healing > 0.0 && world.units[&2].hp > before);

        let buff = defs
            .iter()
            .find(|def| def.id == "AHds")
            .expect("AHds");
        let outcome = apply_effect(buff, 1, EffectTarget::Unit(1), &mut world).expect("buff");
        assert_eq!(outcome.buffs_applied, 1);
        assert!(!world.units[&1].buffs.is_empty());

        let summon = defs
            .iter()
            .find(|def| def.effect_kind() == Some(EffectKind::Summon))
            .cloned()
            .unwrap_or_else(|| {
                let mut def = synthetic(EffectKind::Summon, 1.0, 0.0, false);
                def.effect_params.unit_id = Some("hfoo".into());
                def
            });
        let before = world.units.len();
        let outcome = apply_effect(&summon, 1, EffectTarget::None, &mut world).expect("summon");
        assert!(outcome.summons > 0 && world.units.len() > before);

        let movement = defs
            .iter()
            .find(|def| def.effect_kind() == Some(EffectKind::Movement))
            .cloned()
            .unwrap_or_else(|| synthetic(EffectKind::Movement, 25.0, 5.0, false));
        let before = world.units[&1].speed;
        apply_effect(&movement, 1, EffectTarget::Unit(1), &mut world).expect("movement");
        assert!(world.units[&1].speed > before);

        let data_only = defs
            .iter()
            .find(|def| !def.is_executable())
            .expect("data-only row");
        let error = apply_effect(data_only, 1, EffectTarget::None, &mut world).unwrap_err();
        assert!(error.to_string().contains("data-only"), "{error}");
    }
}
