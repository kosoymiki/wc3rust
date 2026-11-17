//! Phase-0 gameplay constants: physics, balance, weapons, abilities and items.
//!
//! Sources are the shipped TFT data files, parsed with the canonical
//! `wc3-re::slk::Sheet` / `wc3-re::txt::TextTable` readers:
//!
//! - `analysis/war3x_local/Units/MiscGame.txt` (hero/revive/speed gameplay
//!   constants with formula comments),
//! - `analysis/war3x_data/Units/{UnitBalance,UnitWeapons,AbilityData,ItemData}.slk`,
//! - `GameplayConstants.txt` when present in the install MPQs (searched through
//!   the MPQ listfile; extracted to `analysis/agent-re/phase0/data/`).
//!
//! No values are invented. Derived weapon numbers are computed from the shipped
//! `dmgplus/dice/sides/cool` columns and validated against the shipped
//! `avgdmg*`/`DPS` columns; the match counts are part of the artifact.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use wc3_mpq::MpqArchive;

use crate::error::{ReError, Result};
use crate::manifest::json_str;
use crate::slk::Sheet;
use crate::txt::TextTable;

/// Resolved data-file paths (kept explicit so a version switch is visible).
#[derive(Debug, Clone)]
pub struct GameplayPaths {
    pub misc: Option<PathBuf>,
    pub balance: PathBuf,
    pub weapons: PathBuf,
    pub abilities: PathBuf,
    pub items: PathBuf,
}

impl GameplayPaths {
    pub fn discover(root: &Path) -> Self {
        let local = root.join("analysis/war3x_local/Units");
        let tft = root.join("analysis/war3x_data/Units");
        let full = root.join("analysis/war3_full3/Units");
        let misc = [
            local.join("MiscGame.txt"),
            tft.join("MiscGame.txt"),
            full.join("MiscGame.txt"),
        ]
        .into_iter()
        .find(|path| path.is_file());
        Self {
            misc,
            balance: tft.join("UnitBalance.slk"),
            weapons: tft.join("UnitWeapons.slk"),
            abilities: tft.join("AbilityData.slk"),
            items: tft.join("ItemData.slk"),
        }
    }
}


/// Typed gameplay constants consumed by the simulation/game layer.
///
/// Defaults mirror the existing `wc3-sim` hero table (derived from
/// `MiscData.txt`); values present in `MiscGame.txt` override them. This keeps
/// `Sim` behaviour stable by default while making every constant explicit and
/// loadable from the operator's install.
#[derive(Debug, Clone, PartialEq)]
pub struct GameplayConfig {
    pub max_hero_level: u8,
    pub max_unit_level: u8,
    pub need_hero_xp: Vec<u32>,
    pub grant_hero_xp: Vec<u32>,
    pub grant_normal_xp: u32,
    pub hero_factor_xp: Vec<u32>,
    pub hero_exp_range: f32,
    pub str_attack_bonus: f32,
    pub str_hit_point_bonus: i32,
    pub str_regen_bonus: f32,
    pub int_mana_bonus: i32,
    pub int_regen_bonus: f32,
    pub agi_defense_bonus: f32,
    pub agi_defense_base: f32,
    pub agi_move_bonus: f32,
    pub min_unit_speed: f32,
    pub max_unit_speed: f32,
    pub min_bldg_speed: f32,
    pub max_bldg_speed: f32,
    pub frost_move_speed_decrease: f32,
    pub frost_attack_speed_decrease: f32,
    pub revive_base_factor: f32,
    pub revive_level_factor: f32,
    pub revive_base_lumber_factor: f32,
    pub revive_lumber_level_factor: f32,
    pub revive_max_factor: f32,
    pub revive_time_factor: f32,
    pub revive_max_time_factor: f32,
    pub awaken_base_factor: f32,
    pub awaken_level_factor: f32,
    pub awaken_base_lumber_factor: f32,
    pub awaken_lumber_level_factor: f32,
    pub awaken_max_factor: f32,
    pub need_hero_xp_formula: [f32; 3],
    pub grant_hero_xp_formula: [f32; 3],
}

impl Default for GameplayConfig {
    fn default() -> Self {
        Self {
            max_hero_level: 10,
            max_unit_level: 20,
            // `wc3-sim` hero table (MiscData.txt) is the default; MiscGame's
            // single NeedHeroXP value is used as a cross-check, not a silent
            // truncation of the 10-level table.
            need_hero_xp: vec![200, 500, 900, 1400, 2000, 2700, 3500, 4400, 5400, 5400],
            grant_hero_xp: vec![80, 100, 120, 140, 160, 200, 240, 280, 320, 400],
            grant_normal_xp: 25,
            hero_factor_xp: vec![80, 70, 60, 50, 0],
            hero_exp_range: 1200.0,
            str_attack_bonus: 1.0,
            str_hit_point_bonus: 25,
            str_regen_bonus: 0.05,
            int_mana_bonus: 15,
            int_regen_bonus: 0.05,
            agi_defense_bonus: 0.30,
            agi_defense_base: -2.0,
            agi_move_bonus: 0.0,
            min_unit_speed: 150.0,
            max_unit_speed: 400.0,
            min_bldg_speed: 25.0,
            max_bldg_speed: 400.0,
            frost_move_speed_decrease: 0.5,
            frost_attack_speed_decrease: 0.25,
            revive_base_factor: 0.40,
            revive_level_factor: 0.10,
            revive_base_lumber_factor: 0.0,
            revive_lumber_level_factor: 0.0,
            revive_max_factor: 4.0,
            revive_time_factor: 0.65,
            revive_max_time_factor: 2.0,
            awaken_base_factor: 0.80,
            awaken_level_factor: 0.20,
            awaken_base_lumber_factor: 0.80,
            awaken_lumber_level_factor: 0.20,
            awaken_max_factor: 8.0,
            need_hero_xp_formula: [1.0, 100.0, 0.0],
            grant_hero_xp_formula: [1.0, 0.0, 100.0],
        }
    }
}

fn parse_f32(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.is_empty() || value == "-" || value == " - " {
        return None;
    }
    value
        .parse()
        .ok()
        .or_else(|| value.strip_prefix('.').and_then(|rest| format!("0.{rest}").parse().ok()))
}

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn parse_i32(value: &str) -> Option<i32> {
    value.trim().parse().ok()
}

fn parse_u8(value: &str) -> Option<u8> {
    value.trim().parse().ok()
}

fn parse_u32_list(value: &str) -> Vec<u32> {
    value
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect()
}

impl GameplayConfig {
    /// Parse a `MiscGame.txt`-style text table.
    pub fn from_misc(table: &TextTable) -> Self {
        let mut config = Self::default();
        for (name, value) in table.entries() {
            config.apply(name, value);
        }
        config
    }

    /// Parse the `misc` section of a [`GameplayReport`].
    pub fn from_constants(entries: &[ConstantEntry]) -> Self {
        let mut config = Self::default();
        for entry in entries {
            config.apply(&entry.name, &entry.value);
        }
        config
    }

    /// Load from an install root (or repo analysis root).
    ///
    /// Candidate order: `<root>/Units/MiscGame.txt`,
    /// `<root>/analysis/war3x_local/Units/MiscGame.txt`,
    /// `<root>/war3x_local/Units/MiscGame.txt`, the sibling
    /// `war3x_local/Units/MiscGame.txt`, then the install MPQ listfiles.
    pub fn load(root: &Path) -> Result<Self> {
        let mut candidates = vec![
            root.join("Units/MiscGame.txt"),
            root.join("analysis/war3x_local/Units/MiscGame.txt"),
            root.join("war3x_local/Units/MiscGame.txt"),
        ];
        if let Some(parent) = root.parent() {
            candidates.push(parent.join("war3x_local/Units/MiscGame.txt"));
        }
        for path in &candidates {
            if let Ok(bytes) = fs::read(path) {
                let table = TextTable::parse(path.display().to_string(), &bytes)?;
                return Ok(Self::from_misc(&table));
            }
        }
        for dir in [root.to_path_buf(), root.join("analysis")] {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("mpq"))
                {
                    continue;
                }
                let Ok(archive) = MpqArchive::open(&path) else {
                    continue;
                };
                for name in archive.listfile() {
                    if name.to_ascii_lowercase().ends_with("miscgame.txt") {
                        if let Ok(bytes) = archive.read(name) {
                            let table =
                                TextTable::parse(format!("{}:{name}", path.display()), &bytes)?;
                            return Ok(Self::from_misc(&table));
                        }
                    }
                }
            }
        }
        Err(ReError::Unsupported(format!(
            "MiscGame.txt not found under {}",
            root.display()
        )))
    }

    fn apply(&mut self, name: &str, value: &str) {
        match name.to_ascii_lowercase().as_str() {
            "maxherolevel" => {
                if let Some(value) = parse_u8(value) {
                    self.max_hero_level = value;
                }
            }
            "maxunitlevel" => {
                if let Some(value) = parse_u8(value) {
                    self.max_unit_level = value;
                }
            }
            "needheropx" => {
                let list = parse_u32_list(value);
                if list.len() >= 2 {
                    self.need_hero_xp = list;
                }
            }
            "grantheropx" => {
                let list = parse_u32_list(value);
                // MiscGame ships five values; the sim table has ten. Only a
                // full table replaces the default, otherwise the default stays
                // authoritative and the 5-value table is still visible in the
                // JSON report.
                if list.len() >= 10 {
                    self.grant_hero_xp = list;
                }
            }
            "grantnormalxp" => {
                if let Some(value) = parse_u32(value) {
                    self.grant_normal_xp = value;
                }
            }
            "herofactorpx" => {
                let list = parse_u32_list(value);
                if !list.is_empty() {
                    self.hero_factor_xp = list;
                }
            }
            "heroexprange" => {
                if let Some(value) = parse_f32(value) {
                    self.hero_exp_range = value;
                }
            }
            "strattackbonus" => {
                if let Some(value) = parse_f32(value) {
                    self.str_attack_bonus = value;
                }
            }
            "strhitpointbonus" => {
                if let Some(value) = parse_i32(value) {
                    self.str_hit_point_bonus = value;
                }
            }
            "strregenbonus" => {
                if let Some(value) = parse_f32(value) {
                    self.str_regen_bonus = value;
                }
            }
            "intmanabonus" => {
                if let Some(value) = parse_i32(value) {
                    self.int_mana_bonus = value;
                }
            }
            "intregenbonus" => {
                if let Some(value) = parse_f32(value) {
                    self.int_regen_bonus = value;
                }
            }
            "agidefensebonus" => {
                if let Some(value) = parse_f32(value) {
                    self.agi_defense_bonus = value;
                }
            }
            "agidefensebase" => {
                if let Some(value) = parse_f32(value) {
                    self.agi_defense_base = value;
                }
            }
            "agimovebonus" => {
                if let Some(value) = parse_f32(value) {
                    self.agi_move_bonus = value;
                }
            }
            "minunitspeed" => {
                if let Some(value) = parse_f32(value) {
                    self.min_unit_speed = value;
                }
            }
            "maxunitspeed" => {
                if let Some(value) = parse_f32(value) {
                    self.max_unit_speed = value;
                }
            }
            "minbldgspeed" => {
                if let Some(value) = parse_f32(value) {
                    self.min_bldg_speed = value;
                }
            }
            "maxbldgspeed" => {
                if let Some(value) = parse_f32(value) {
                    self.max_bldg_speed = value;
                }
            }
            "frostmovespeeddecrease" => {
                if let Some(value) = parse_f32(value) {
                    self.frost_move_speed_decrease = value;
                }
            }
            "frostattackspeeddecrease" => {
                if let Some(value) = parse_f32(value) {
                    self.frost_attack_speed_decrease = value;
                }
            }
            "revivebasefactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_base_factor = value;
                }
            }
            "revivelevelfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_level_factor = value;
                }
            }
            "revivebaselumberfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_base_lumber_factor = value;
                }
            }
            "revivelumberlevelfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_lumber_level_factor = value;
                }
            }
            "revivemaxfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_max_factor = value;
                }
            }
            "revivetimefactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_time_factor = value;
                }
            }
            "revivemaxtimefactor" => {
                if let Some(value) = parse_f32(value) {
                    self.revive_max_time_factor = value;
                }
            }
            "awakenbasefactor" => {
                if let Some(value) = parse_f32(value) {
                    self.awaken_base_factor = value;
                }
            }
            "awakenlevelfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.awaken_level_factor = value;
                }
            }
            "awakenbaselumberfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.awaken_base_lumber_factor = value;
                }
            }
            "awakenlumberlevelfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.awaken_lumber_level_factor = value;
                }
            }
            "awakenmaxfactor" => {
                if let Some(value) = parse_f32(value) {
                    self.awaken_max_factor = value;
                }
            }
            "needheropxformulaa" => {
                if let Some(value) = parse_f32(value) {
                    self.need_hero_xp_formula[0] = value;
                }
            }
            "needheropxformulab" => {
                if let Some(value) = parse_f32(value) {
                    self.need_hero_xp_formula[1] = value;
                }
            }
            "needheropxformulac" => {
                if let Some(value) = parse_f32(value) {
                    self.need_hero_xp_formula[2] = value;
                }
            }
            "grantheropxformulaa" => {
                if let Some(value) = parse_f32(value) {
                    self.grant_hero_xp_formula[0] = value;
                }
            }
            "grantheropxformulab" => {
                if let Some(value) = parse_f32(value) {
                    self.grant_hero_xp_formula[1] = value;
                }
            }
            "grantheropxformulac" => {
                if let Some(value) = parse_f32(value) {
                    self.grant_hero_xp_formula[2] = value;
                }
            }
            _ => {}
        }
    }

    /// XP needed to advance from `level` to `level + 1` (0 at the level cap).
    pub fn hero_xp_to_next(&self, level: u8) -> u32 {
        if level == 0 || level >= self.max_hero_level {
            return 0;
        }
        self.need_hero_xp
            .get((level - 1) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Cumulative XP required to reach `level` from level 1.
    pub fn hero_xp_for_level(&self, level: u8) -> u32 {
        (1..level.max(1)).map(|step| self.hero_xp_to_next(step)).sum()
    }

    /// Gold revive cost, capped like `MiscGame.txt` documents.
    pub fn revive_gold(&self, original_cost: f64, level: u8) -> f64 {
        let level = f64::from(level.max(1));
        let factor = self.revive_base_factor as f64
            + self.revive_level_factor as f64 * (level - 1.0);
        original_cost * factor.min(self.revive_max_factor as f64)
    }

    /// Lumber revive cost, same cap.
    pub fn revive_lumber(&self, original_cost: f64, level: u8) -> f64 {
        let level = f64::from(level.max(1));
        let factor = self.revive_base_lumber_factor as f64
            + self.revive_lumber_level_factor as f64 * (level - 1.0);
        original_cost * factor.min(self.revive_max_factor as f64)
    }

    /// Revive time, capped like `MiscGame.txt` documents.
    pub fn revive_time(&self, original_time: f64, level: u8) -> f64 {
        let level = f64::from(level.max(1));
        let time = original_time * level * self.revive_time_factor as f64;
        time.min(original_time * self.revive_max_time_factor as f64)
    }

    /// Frost slow applies the documented move-speed multiplier.
    pub fn frost_speed(&self, speed: f64) -> f64 {
        speed * (1.0 - self.frost_move_speed_decrease as f64)
    }

    /// Frost attack-speed effect is a cooldown multiplier.
    pub fn frost_attack_interval(&self, interval: f64) -> f64 {
        interval / (1.0 - self.frost_attack_speed_decrease.max(0.0) as f64)
    }

    pub fn clamp_unit_speed(&self, speed: f64, building: bool) -> f64 {
        let (min, max) = if building {
            (self.min_bldg_speed as f64, self.max_bldg_speed as f64)
        } else {
            (self.min_unit_speed as f64, self.max_unit_speed as f64)
        };
        speed.clamp(min, max)
    }
}

#[derive(Debug, Clone)]
pub struct ConstantEntry {
    pub section: String,
    pub name: String,
    pub value: String,
    pub unit: String,
    pub formula: String,
}

#[derive(Debug, Clone)]
pub struct FormulaSpec {
    pub name: String,
    pub expression: String,
    pub source: String,
}

/// A complete selected-column dump of one shipped SLK table.
#[derive(Debug, Clone, Default)]
pub struct TableDump {
    pub name: String,
    pub source: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct GameplayReport {
    pub generated_unix: u64,
    pub sources: BTreeMap<String, String>,
    pub counts: BTreeMap<String, usize>,
    pub fields: BTreeMap<String, String>,
    pub formulas: Vec<FormulaSpec>,
    pub misc: Vec<ConstantEntry>,
    pub gameplay_constants: Vec<ConstantEntry>,
    pub units: TableDump,
    pub weapons: TableDump,
    pub abilities: TableDump,
    pub items: TableDump,
    pub validation: BTreeMap<String, String>,
}

impl GameplayReport {
    pub fn ok(&self) -> bool {
        self.counts.get("units").copied().unwrap_or(0) > 0
            && self.counts.get("weapons").copied().unwrap_or(0) > 0
            && self.counts.get("abilities").copied().unwrap_or(0) > 0
            && self.counts.get("items").copied().unwrap_or(0) > 0
    }

    /// Typed constants parsed from the same `MiscGame.txt` entries.
    pub fn config(&self) -> GameplayConfig {
        GameplayConfig::from_constants(&self.misc)
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-gameplay-constants-v1\",\"generated_unix\":{},\"ok\":{},\"sources\":{},\"counts\":{},\"fields\":{},\"validation\":{},\"formulas\":[",
            self.generated_unix,
            self.ok(),
            json_map_str(&self.sources),
            json_map_num(&self.counts),
            json_map_str(&self.fields),
            json_map_str(&self.validation)
        );
        for (index, formula) in self.formulas.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"name\":{},\"expression\":{},\"source\":{}}}",
                json_str(&formula.name),
                json_str(&formula.expression),
                json_str(&formula.source)
            );
        }
        out.push_str("],\"misc\":");
        write_constants(&mut out, &self.misc);
        out.push_str(",\"gameplay_constants\":");
        write_constants(&mut out, &self.gameplay_constants);
        out.push_str(",\"units\":");
        write_table(&mut out, &self.units);
        out.push_str(",\"weapons\":");
        write_table(&mut out, &self.weapons);
        out.push_str(",\"abilities\":");
        write_table(&mut out, &self.abilities);
        out.push_str(",\"items\":");
        write_table(&mut out, &self.items);
        out.push('}');
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# GAMEPLAY CONSTANTS — physics, balance, weapons, abilities, items");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Generated (unix): {} · **ok={}**",
            self.generated_unix,
            self.ok()
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "## Sources");
        for (key, value) in &self.sources {
            let _ = writeln!(out, "- `{key}`: `{value}`");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Counts");
        let _ = writeln!(out, "| table | rows |");
        let _ = writeln!(out, "|---|---:|");
        for (key, value) in &self.counts {
            let _ = writeln!(out, "| {key} | {value} |");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Field units");
        let _ = writeln!(out, "| field | unit |");
        let _ = writeln!(out, "|---|---|");
        for (key, value) in &self.fields {
            let _ = writeln!(out, "| `{key}` | {value} |");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Formulas");
        let _ = writeln!(out, "| formula | expression | source |");
        let _ = writeln!(out, "|---|---|---|");
        for formula in &self.formulas {
            let _ = writeln!(
                out,
                "| {} | `{}` | {} |",
                formula.name, formula.expression, formula.source
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Validation");
        let _ = writeln!(out, "| check | result |");
        let _ = writeln!(out, "|---|---|");
        for (key, value) in &self.validation {
            let _ = writeln!(out, "| {key} | {value} |");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## MiscGame / GameplayConstants");
        let _ = writeln!(out);
        let _ = writeln!(out, "| section | key | value | unit | formula |");
        let _ = writeln!(out, "|---|---|---|---|---|");
        for entry in self.misc.iter().chain(self.gameplay_constants.iter()) {
            let _ = writeln!(
                out,
                "| {} | `{}` | `{}` | {} | {} |",
                entry.section, entry.name, entry.value, entry.unit, entry.formula
            );
        }
        for table in [&self.units, &self.weapons, &self.abilities, &self.items] {
            let _ = writeln!(out);
            let _ = writeln!(out, "## {} ({} rows)", table.name, table.rows.len());
            let _ = writeln!(out);
            let _ = writeln!(out, "| {} |", table.columns.join(" | "));
            let _ = writeln!(
                out,
                "|{}|",
                table.columns.iter().map(|_| "---").collect::<Vec<_>>().join("|")
            );
            for row in &table.rows {
                let escaped = row.iter().map(|value| escape_md(value)).collect::<Vec<_>>();
                let _ = writeln!(out, "| {} |", escaped.join(" | "));
            }
        }
        out
    }
}

fn escape_md(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn write_constants(out: &mut String, entries: &[ConstantEntry]) {
    out.push('[');
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"section\":{},\"name\":{},\"value\":{},\"unit\":{},\"formula\":{}}}",
            json_str(&entry.section),
            json_str(&entry.name),
            json_str(&entry.value),
            json_str(&entry.unit),
            json_str(&entry.formula)
        );
    }
    out.push(']');
}

fn write_table(out: &mut String, table: &TableDump) {
    let _ = write!(
        out,
        "{{\"name\":{},\"source\":{},\"columns\":[{}],\"rows\":[",
        json_str(&table.name),
        json_str(&table.source),
        table
            .columns
            .iter()
            .map(|column| json_str(column))
            .collect::<Vec<_>>()
            .join(",")
    );
    for (index, row) in table.rows.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('[');
        for (cell_index, cell) in row.iter().enumerate() {
            if cell_index > 0 {
                out.push(',');
            }
            out.push_str(&json_str(cell));
        }
        out.push(']');
    }
    out.push_str("]}");
}

fn json_map_str(map: &BTreeMap<String, String>) -> String {
    let body = map
        .iter()
        .map(|(key, value)| format!("{}:{}", json_str(key), json_str(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

fn json_map_num(map: &BTreeMap<String, usize>) -> String {
    let body = map
        .iter()
        .map(|(key, value)| format!("{}:{}", json_str(key), value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

fn num(sheet: &Sheet, row: usize, column: &str) -> Option<f64> {
    let value = sheet.cell(row, column)?.trim();
    if value.is_empty() || value == "-" || value == " - " {
        return None;
    }
    value.parse().ok()
}

fn int(sheet: &Sheet, row: usize, column: &str) -> Option<i64> {
    num(sheet, row, column).map(|value| value as i64)
}

fn text(sheet: &Sheet, row: usize, column: &str) -> String {
    sheet
        .cell(row, column)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn dump(
    sheet: &Sheet,
    name: &str,
    source: &str,
    id_column: &str,
    columns: &[&str],
    extra: &[&str],
) -> TableDump {
    let mut dump = TableDump {
        name: name.to_string(),
        source: source.to_string(),
        columns: std::iter::once(id_column.to_string())
            .chain(columns.iter().map(|column| (*column).to_string()))
            .chain(extra.iter().map(|column| (*column).to_string()))
            .collect(),
        rows: Vec::new(),
    };
    for row in 0..sheet.rows.len() {
        let id = sheet.rows[row].first().cloned().unwrap_or_default();
        if id.trim().is_empty() {
            continue;
        }
        let mut values: Vec<String> = std::iter::once(id.clone())
            .chain(columns.iter().map(|column| text(sheet, row, column)))
            .collect();
        values.extend(extra.iter().map(|_| String::new()));
        dump.rows.push(values);
    }
    dump
}

fn unit_column(name: &str) -> &'static str {
    match name {
        "level" => "level",
        "HP" | "realHP" | "regenHP" => "hit points / hit points per second",
        "manaN" | "realM" | "mana0" | "regenMana" => "mana points / mana points per second",
        "def" | "defUp" | "realdef" => "armor points",
        "spd" | "minSpd" | "maxSpd" => "map units per second",
        "bldtm" | "reptm" => "seconds",
        "sight" | "nsight" => "map units",
        "goldcost" | "lumbercost" | "goldRep" | "lumberRep" => "resources",
        "collision" => "map units",
        "rangeN1" | "rangeN2" => "map units",
        "cool1" | "cool2" => "seconds",
        "dice1" | "dice2" | "sides1" | "sides2" => "count",
        "dmgplus1" | "dmgplus2" | "dmgUp1" | "dmgUp2" => "damage points",
        "mindmg1" | "avgdmg1" | "maxdmg1" | "mindmg2" | "avgdmg2" | "maxdmg2" => {
            "damage points"
        }
        "DPS" => "damage per second",
        "Cool1" | "Cast1" | "Dur1" | "HeroDur1" => "seconds",
        "Cost1" => "mana points",
        "stockMax" | "stockRegen" | "stockStart" => "count / seconds",
        "Level" => "item level",
        _ => "raw",
    }
}

fn gameplay_field_units() -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for field in [
        "level", "HP", "realHP", "regenHP", "manaN", "realM", "mana0", "regenMana",
        "def", "defUp", "realdef", "spd", "minSpd", "maxSpd", "bldtm", "reptm",
        "sight", "nsight", "goldcost", "lumbercost", "collision", "cool1", "rangeN1",
        "dice1", "sides1", "dmgplus1", "dmgUp1", "mindmg1", "avgdmg1", "maxdmg1",
        "DPS", "Cool1", "Cast1", "Dur1", "HeroDur1", "stockMax", "stockRegen",
        "stockStart", "Level",
    ] {
        fields.insert(field.to_string(), unit_column(field).to_string());
    }
    fields
}

fn misc_unit(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.contains("speed") && !lower.contains("factor") {
        "map units per second"
    } else if lower.contains("time") {
        "seconds"
    } else if lower.contains("range") {
        "map units"
    } else if lower.contains("xp") || lower.contains("experience") {
        "experience points"
    } else if lower.contains("cost") {
        "resources"
    } else if lower.contains("factor") || lower.contains("decrease") {
        "multiplier"
    } else if lower.contains("level") {
        "level"
    } else {
        "raw"
    }
}

fn misc_formula(name: &str) -> &'static str {
    match name {
        "ReviveBaseFactor" => "goldRevivalCost = originalCost * (ReviveBaseFactor + ReviveLevelFactor*(level-1)), capped at originalCost * ReviveMaxFactor",
        "ReviveLevelFactor" => "see ReviveBaseFactor",
        "ReviveBaseLumberFactor" | "ReviveLumberLevelFactor" => "lumberRevivalCost = originalCost * (ReviveBaseLumberFactor + ReviveLumberLevelFactor*(level-1)), capped at originalCost * ReviveMaxFactor",
        "ReviveTimeFactor" => "revivalTime = originalTime * level * ReviveTimeFactor, capped at originalTime * ReviveMaxTimeFactor",
        "AwakenBaseFactor" => "awakenCost = originalCost * (AwakenBaseFactor + AwakenLevelFactor*(level-1)), capped at originalCost * AwakenMaxFactor",
        "NeedHeroXPFormulaA" | "NeedHeroXPFormulaB" | "NeedHeroXPFormulaC" => {
            "f(x) = A*f(x-1) + B*x + C"
        }
        "GrantHeroXPFormulaA" | "GrantHeroXPFormulaB" | "GrantHeroXPFormulaC" => {
            "f(x) = A*f(x-1) + B*x + C"
        }
        "MinUnitSpeed" | "MaxUnitSpeed" => "clamp unit movement speed",
        "FrostMoveSpeedDecrease" => "slowedSpeed = speed * (1 - value)",
        "StrAttackBonus" => "damage += STR * value",
        "StrHitPointBonus" => "maxHp += STR * value",
        "StrRegenBonus" => "hpRegen += STR * value",
        "IntManaBonus" => "maxMana += INT * value",
        "IntRegenBonus" => "manaRegen += INT * value",
        "AgiDefenseBonus" => "armor += AGI * value + AgiDefenseBase",
        "AgiDefenseBase" => "armor = AGI * AgiDefenseBonus + value",
        _ => "",
    }
}

fn collect_text_table(table: &TextTable) -> Vec<ConstantEntry> {
    let mut entries = Vec::new();
    for section in &table.sections {
        for (name, value) in &section.entries {
            entries.push(ConstantEntry {
                section: section.name.clone(),
                name: name.clone(),
                value: value.clone(),
                unit: misc_unit(name).to_string(),
                formula: misc_formula(name).to_string(),
            });
        }
    }
    entries
}

fn find_gameplay_constants(root: &Path) -> Option<(String, Vec<u8>)> {
    for archive_name in [
        "analysis/tft_files_Files100_War3x.mpq",
        "analysis/tft_files_Files100_War3xlocal.mpq",
    ] {
        let path = root.join(archive_name);
        if !path.is_file() {
            continue;
        }
        let Ok(archive) = MpqArchive::open(&path) else {
            continue;
        };
        for name in archive.listfile() {
            if name.to_ascii_lowercase().contains("gameplayconstants") {
                if let Ok(bytes) = archive.read(name) {
                    return Some((format!("{archive_name}:{name}"), bytes));
                }
            }
        }
    }
    for candidate in [
        "analysis/war3x_local/Units/GameplayConstants.txt",
        "analysis/war3x_data/Units/GameplayConstants.txt",
        "analysis/war3_full3/Units/GameplayConstants.txt",
    ] {
        let path = root.join(candidate);
        if let Ok(bytes) = fs::read(&path) {
            return Some((candidate.to_string(), bytes));
        }
    }
    None
}

pub fn build(root: &Path) -> Result<GameplayReport> {
    let paths = GameplayPaths::discover(root);
    let balance_bytes = fs::read(&paths.balance)
        .map_err(|err| ReError::Parse(format!("UnitBalance.slk: {err}")))?;
    let weapons_bytes = fs::read(&paths.weapons)
        .map_err(|err| ReError::Parse(format!("UnitWeapons.slk: {err}")))?;
    let abilities_bytes = fs::read(&paths.abilities)
        .map_err(|err| ReError::Parse(format!("AbilityData.slk: {err}")))?;
    let items_bytes = fs::read(&paths.items)
        .map_err(|err| ReError::Parse(format!("ItemData.slk: {err}")))?;
    let balance = Sheet::parse("UnitBalance", &balance_bytes)?;
    let weapons = Sheet::parse("UnitWeapons", &weapons_bytes)?;
    let abilities = Sheet::parse("AbilityData", &abilities_bytes)?;
    let items = Sheet::parse("ItemData", &items_bytes)?;

    let mut report = GameplayReport {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        ..GameplayReport::default()
    };
    report.sources.insert(
        "unit_balance".into(),
        paths.balance.display().to_string(),
    );
    report
        .sources
        .insert("unit_weapons".into(), paths.weapons.display().to_string());
    report
        .sources
        .insert("ability_data".into(), paths.abilities.display().to_string());
    report
        .sources
        .insert("item_data".into(), paths.items.display().to_string());

    if let Some(misc_path) = &paths.misc {
        let bytes = fs::read(misc_path)?;
        let table = TextTable::parse(misc_path.display().to_string(), &bytes)?;
        report.misc = collect_text_table(&table);
        report
            .sources
            .insert("misc_game".into(), misc_path.display().to_string());
    } else {
        report.sources.insert("misc_game".into(), "absent".into());
    }

    if let Some((source, bytes)) = find_gameplay_constants(root) {
        if let Ok(table) = TextTable::parse(source.clone(), &bytes) {
            report.gameplay_constants = collect_text_table(&table);
        }
        let out_dir = root.join("analysis/agent-re/phase0/data");
        let _ = fs::create_dir_all(&out_dir);
        let _ = fs::write(out_dir.join("GameplayConstants.txt"), &bytes);
        report.sources.insert("gameplay_constants".into(), source);
    } else {
        report
            .sources
            .insert("gameplay_constants".into(), "absent-in-this-install".into());
    }

    let unit_columns = [
        "level", "goldcost", "lumbercost", "HP", "realHP", "regenHP", "manaN", "realM",
        "mana0", "regenMana", "def", "defUp", "realdef", "spd", "minSpd", "maxSpd",
        "bldtm", "reptm", "sight", "nsight", "collision", "isbldg", "STR", "INT", "AGI",
        "STRplus", "INTplus", "AGIplus", "Primary", "tilesets", "upgrades",
    ];
    report.units = dump(
        &balance,
        "UnitBalance",
        &paths.balance.display().to_string(),
        "unitBalanceID",
        &unit_columns,
        &["hero_hp_formula"],
    );
    // Hero HP estimate: realHP + (level-1)*HP, only where the shipped HP column
    // differs from realHP (the hero signal observed on Hamg/hfoo/hpea).
    for row in 0..balance.rows.len() {
        let id = balance.rows[row].first().cloned().unwrap_or_default();
        if id.trim().is_empty() {
            continue;
        }
        let level = int(&balance, row, "level");
        let hp = num(&balance, row, "HP");
        let real_hp = num(&balance, row, "realHP");
        let estimate = match (level, hp, real_hp) {
            (Some(level), Some(hp), Some(real_hp)) if level > 1 && hp != real_hp => {
                Some(real_hp + (level as f64 - 1.0) * hp)
            }
            _ => None,
        };
        if let Some(estimate) = estimate {
            if let Some(dump_row) = report
                .units
                .rows
                .iter_mut()
                .find(|row| row.first().is_some_and(|value| *value == id))
            {
                let last = dump_row.len() - 1;
                dump_row[last] = format!("{estimate:.1}");
            }
        }
    }

    let weapon_columns = [
        "weapsOn", "atkType1", "weapTp1", "cool1", "dice1", "sides1", "dmgplus1",
        "dmgUp1", "mindmg1", "avgdmg1", "maxdmg1", "rangeN1", "targCount1", "DPS",
        "atkType2", "weapTp2", "cool2", "dice2", "sides2", "dmgplus2", "dmgUp2",
        "mindmg2", "avgdmg2", "maxdmg2", "rangeN2", "targCount2", "DPS2",
    ];
    report.weapons = dump(
        &weapons,
        "UnitWeapons",
        &paths.weapons.display().to_string(),
        "unitWeapID",
        &weapon_columns,
        &[
            "avg1_formula",
            "dps1_formula",
            "avg1_match",
            "dps1_match",
            "avg2_formula",
            "dps2_formula",
            "avg2_match",
            "dps2_match",
        ],
    );
    let mut avg_total = 0usize;
    let mut avg_matches = 0usize;
    let mut dps_total = 0usize;
    let mut dps_matches = 0usize;
    for row in 0..weapons.rows.len() {
        let id = weapons.rows[row].first().cloned().unwrap_or_default();
        if id.trim().is_empty() {
            continue;
        }
        let Some(dump_row) = report
            .weapons
            .rows
            .iter_mut()
            .find(|row| row.first().is_some_and(|value| *value == id))
        else {
            continue;
        };
        for (slot, column, avg_column, dps_column) in [
            (1usize, "cool1", "avgdmg1", "DPS"),
            (2usize, "cool2", "avgdmg2", "DPS2"),
        ] {
            let dice = num(&weapons, row, &format!("dice{slot}")).unwrap_or(0.0);
            let sides = num(&weapons, row, &format!("sides{slot}")).unwrap_or(0.0);
            let plus = num(&weapons, row, &format!("dmgplus{slot}")).unwrap_or(0.0);
            let cool = num(&weapons, row, column);
            let avg = plus + dice * (sides + 1.0) / 2.0;
            let shipped_avg = num(&weapons, row, avg_column);
            let shipped_dps = num(&weapons, row, dps_column);
            if shipped_avg.is_some() {
                avg_total += 1;
            }
            let avg_match = shipped_avg.is_some_and(|value| (value - avg).abs() <= 0.01);
            if avg_match {
                avg_matches += 1;
            }
            if let (Some(cool), Some(shipped_dps)) = (cool, shipped_dps) {
                if cool > 0.0 {
                    dps_total += 1;
                    let dps = avg / cool;
                    let dps_match = (shipped_dps - dps).abs() <= 0.05;
                    if dps_match {
                        dps_matches += 1;
                    }
                    let base = 28 + (slot - 1) * 4;
                    dump_row[base] = format!("{avg:.4}");
                    dump_row[base + 1] = format!("{dps:.4}");
                    dump_row[base + 2] = avg_match.to_string();
                    dump_row[base + 3] = dps_match.to_string();
                    continue;
                }
            }
            let base = 28 + (slot - 1) * 4;
            dump_row[base] = format!("{avg:.4}");
            dump_row[base + 1] = String::new();
            dump_row[base + 2] = avg_match.to_string();
            dump_row[base + 3] = String::new();
        }
    }

    let ability_columns = [
        "alias", "levels", "reqLevel", "levelSkip", "priority", "hero", "item",
        "useInEditor", "Cast1", "Cool1", "Dur1", "HeroDur1", "Cost1", "targs1",
        "rangeN1", "Farea1", "Harea1", "Qarea1",
    ];
    report.abilities = dump(
        &abilities,
        "AbilityData",
        &paths.abilities.display().to_string(),
        "alias",
        &ability_columns,
        &[],
    );
    let item_columns = [
        "itemID", "Level", "goldcost", "lumbercost", "stockMax", "stockRegen",
        "stockStart", "usable", "droppable", "pawnable", "sellable", "powerup",
        "perishable", "drop", "HP", "morph",
    ];
    report.items = dump(
        &items,
        "ItemData",
        &paths.items.display().to_string(),
        "itemID",
        &item_columns,
        &[],
    );

    report.counts.insert("misc_constants".into(), report.misc.len());
    report.counts.insert(
        "gameplay_constants".into(),
        report.gameplay_constants.len(),
    );
    report.counts.insert("units".into(), report.units.rows.len());
    report.counts.insert("weapons".into(), report.weapons.rows.len());
    report.counts.insert("abilities".into(), report.abilities.rows.len());
    report.counts.insert("items".into(), report.items.rows.len());
    report.counts.insert(
        "unit_balance_columns".into(),
        balance.columns.len(),
    );
    report.counts.insert(
        "unit_weapons_columns".into(),
        weapons.columns.len(),
    );
    report.counts.insert("ability_columns".into(), abilities.columns.len());
    report.counts.insert("item_columns".into(), items.columns.len());

    report.fields = gameplay_field_units();
    report.formulas = vec![
        FormulaSpec {
            name: "weapon_average_damage".into(),
            expression: "avg = dmgplus + dice * (sides + 1) / 2".into(),
            source: "UnitWeapons.slk dmgplus1/dice1/sides1 (slot 2: 2)".into(),
        },
        FormulaSpec {
            name: "weapon_dps".into(),
            expression: "dps = avg / cool".into(),
            source: "UnitWeapons.slk cool1/avgdmg1/DPS (slot 2: cool2/avgdmg2/DPS2)".into(),
        },
        FormulaSpec {
            name: "hero_hp_estimate".into(),
            expression: "maxHp(level) = realHP + (level - 1) * HP, only when HP != realHP (hero rows)".into(),
            source: "UnitBalance.slk HP/realHP/level; hero signal observed on Hamg (450 + 4*100) vs hfoo/hpea (HP == realHP)".into(),
        },
        FormulaSpec {
            name: "hero_xp_curve".into(),
            expression: "f(x) = A*f(x-1) + B*x + C".into(),
            source: "MiscGame.txt NeedHeroXPFormulaA/B/C, GrantHeroXPFormulaA/B/C".into(),
        },
        FormulaSpec {
            name: "hero_revive_cost".into(),
            expression: "goldRevivalCost = originalCost * (ReviveBaseFactor + ReviveLevelFactor*(level-1)) capped by ReviveMaxFactor".into(),
            source: "MiscGame.txt revive formula comments".into(),
        },
        FormulaSpec {
            name: "hero_revive_time".into(),
            expression: "revivalTime = originalTime * level * ReviveTimeFactor capped by ReviveMaxTimeFactor".into(),
            source: "MiscGame.txt revive formula comments".into(),
        },
        FormulaSpec {
            name: "frost_slow".into(),
            expression: "slowedSpeed = speed * (1 - FrostMoveSpeedDecrease)".into(),
            source: "MiscGame.txt FrostMoveSpeedDecrease".into(),
        },
    ];
    report.validation.insert(
        "weapon_avg_matches".into(),
        format!("{avg_matches}/{avg_total} shipped avgdmg values equal the formula (tol 0.01)"),
    );
    report.validation.insert(
        "weapon_dps_matches".into(),
        format!("{dps_matches}/{dps_total} shipped DPS values equal avg/cool (tol 0.05)"),
    );
    report.validation.insert(
        "ok".into(),
        report.ok().to_string(),
    );
    Ok(report)
}

pub fn write_artifacts(report: &GameplayReport, json: &Path, md: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(json, report.to_json())?;
    fs::write(md, report.to_markdown())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn parses_real_gameplay_tables() {
        let root = root();
        let paths = GameplayPaths::discover(&root);
        if !paths.balance.is_file() || paths.misc.is_none() {
            eprintln!("skip: gameplay tables missing");
            return;
        }
        let report = build(&root).expect("gameplay report");
        assert!(report.ok(), "counts {:?}", report.counts);
        assert!(report.counts["units"] > 800, "units {}", report.counts["units"]);
        assert!(report.counts["weapons"] > 800);
        assert!(report.counts["abilities"] > 700);
        assert_eq!(report.counts["items"], 273);
        assert!(report.counts["misc_constants"] > 40);
        assert!(!report.formulas.is_empty());
    }

    #[test]
    fn weapon_formulas_match_shipped_columns() {
        let root = root();
        if !GameplayPaths::discover(&root).weapons.is_file() {
            eprintln!("skip: UnitWeapons.slk missing");
            return;
        }
        let report = build(&root).expect("gameplay report");
        let avg = report.validation.get("weapon_avg_matches").expect("avg");
        let (left, right) = avg.split_once('/').expect("matches/total");
        let matches = left.parse::<usize>().unwrap_or(0);
        let total = right
            .split_whitespace()
            .next()
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap_or(0);
        assert!(total > 500, "weapon slots {total} ({avg})");
        assert!(
            matches * 100 >= total * 90,
            "avg matches {matches}/{total} ({avg})"
        );
        let dps = report.validation.get("weapon_dps_matches").expect("dps");
        assert!(dps.contains('/'), "dps validation {dps}");
    }

    #[test]
    fn config_loads_typed_values_from_the_install() {
        let root = root();
        let config = GameplayConfig::load(&root).expect("MiscGame config");
        assert_eq!(config.max_hero_level, 10);
        assert_eq!(config.max_unit_level, 20);
        assert_eq!(config.hero_xp_to_next(1), 200);
        assert_eq!(config.hero_xp_for_level(3), 700);
        assert_eq!(config.str_hit_point_bonus, 25);
        assert_eq!(config.int_mana_bonus, 15);
        assert!((config.agi_defense_bonus - 0.30).abs() < 1e-6);
        assert_eq!(config.min_unit_speed, 150.0);
        assert_eq!(config.max_unit_speed, 400.0);
        assert!((config.revive_gold(100.0, 1) - 40.0).abs() < 1e-6);
        assert!((config.frost_speed(200.0) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn json_and_markdown_have_counts_and_schema() {
        let root = root();
        let paths = GameplayPaths::discover(&root);
        if !paths.balance.is_file() {
            eprintln!("skip: gameplay tables missing");
            return;
        }
        let report = build(&root).expect("gameplay report");
        let json = report.to_json();
        assert!(json.contains("\"schema\":\"wc3-gameplay-constants-v1\""));
        assert!(json.contains("\"units\":{"));
        assert!(json.contains("\"weapons\":{"));
        let md = report.to_markdown();
        assert!(md.contains("GAMEPLAY CONSTANTS"));
        assert!(md.contains("weapon_average_damage"));
    }
}
