//! Validation: every row of every table, joins across sheets, references and
//! archive path existence.
//!
//! The report is data, not log text: tools can filter by [`Severity`] and by
//! issue code, and each issue names the table and object it came from.

use wc3_formats::gamedata::{MetaKind, ObjectKind};

use crate::source::Listfile;
use crate::table::{Table, TableKind};
use crate::GameData;

/// Issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Data loss or contradiction in the shipped tables.
    Error,
    /// Suspicious value that the game tolerates (e.g. an icon typo).
    Warning,
    /// Expected shape of the data, reported for completeness.
    Info,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub code: &'static str,
    pub table: String,
    pub id: String,
    pub detail: String,
}

impl Issue {
    fn new(
        severity: Severity,
        code: &'static str,
        table: impl Into<String>,
        id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code,
            table: table.into(),
            id: id.into(),
            detail: detail.into(),
        }
    }
}

/// Row/column statistics for one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStat {
    pub kind: TableKind,
    pub rows: usize,
    pub columns: usize,
    pub empty_ids: usize,
    pub duplicate_ids: usize,
    pub shadow_columns: usize,
    pub overlay_cells: usize,
}

/// Path existence statistics for one reference class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStat {
    pub class: &'static str,
    pub checked: usize,
    pub resolved: usize,
    pub missing: usize,
    /// Values that are command aliases rather than file paths
    /// (`Art=CommandMove`), so they are not file existence checks.
    pub identifiers: usize,
    pub samples: Vec<String>,
}

impl PathStat {
    fn new(class: &'static str) -> Self {
        Self {
            class,
            checked: 0,
            resolved: 0,
            missing: 0,
            identifiers: 0,
            samples: Vec::new(),
        }
    }

    fn record_identifier(&mut self) {
        self.identifiers += 1;
    }

    fn record(&mut self, id: &str, path: &str, resolved: bool) {
        self.checked += 1;
        if resolved {
            self.resolved += 1;
        } else {
            self.missing += 1;
            if self.samples.len() < 8 {
                self.samples.push(format!("{id}: {path}"));
            }
        }
    }
}

/// Archive catalogue statistics (`Buildings\*.mdx` = 131 in the RoC listfile,
/// `ReplaceableTextures\CommandButtons\*.blp` = 691).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogStat {
    pub origin: String,
    pub entries: usize,
    pub paths: usize,
    pub unit_models: usize,
    pub building_models: usize,
    pub command_button_icons: usize,
}

/// Full validation report.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub root: String,
    pub tables: Vec<TableStat>,
    pub issues: Vec<Issue>,
    pub paths: Vec<PathStat>,
    pub catalog: Option<CatalogStat>,
    pub object_data: Vec<(ObjectKind, usize, usize)>,
    pub merged_cells: usize,
}

impl Report {
    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
    }

    pub fn error_count(&self) -> usize {
        self.errors().count()
    }

    pub fn warning_count(&self) -> usize {
        self.warnings().count()
    }

    pub fn ok(&self) -> bool {
        self.error_count() == 0
    }

    /// Total rows validated across all tables.
    pub fn total_rows(&self) -> usize {
        self.tables.iter().map(|stat| stat.rows).sum()
    }

    pub fn path(&self, class: &str) -> Option<&PathStat> {
        self.paths.iter().find(|stat| stat.class == class)
    }
}

/// Columns that must parse as numbers, per table. Every cell of every row is
/// checked; `-`/`_`/empty count as absent and are skipped.
fn numeric_columns(kind: TableKind) -> &'static [&'static str] {
    match kind {
        TableKind::UnitData => &["turnRate", "moveHeight", "collision", "cargoSize", "formation"],
        TableKind::UnitBalance => &[
            "level",
            "goldcost",
            "lumbercost",
            "bldtm",
            "fused",
            "fmade",
            "HP",
            "regenHP",
            "manaN",
            "mana0",
            "regenMana",
            "def",
            "defUp",
            "spd",
            "sight",
            "nsight",
            "STR",
            "INT",
            "AGI",
            "STRplus",
            "INTplus",
            "AGIplus",
            "bountydice",
            "bountysides",
            "bountyplus",
            "stockMax",
            "stockRegen",
            "stockStart",
        ],
        TableKind::UnitWeapons => &[
            "acquire",
            "minRange",
            "rangeN1",
            "cool1",
            "dice1",
            "sides1",
            "dmgplus1",
            "backSw1",
            "dmgpt1",
            "Farea1",
            "Harea1",
            "Qarea1",
            "targCount1",
            "rangeN2",
            "cool2",
            "dice2",
            "sides2",
            "dmgplus2",
            "backSw2",
            "dmgpt2",
            "Farea2",
            "Harea2",
            "Qarea2",
            "targCount2",
        ],
        TableKind::UnitUi => &["scale", "modelScale", "red", "green", "blue", "selZ", "occH"],
        TableKind::UnitAbilities => &[],
        TableKind::AbilityData => &[
            "levels", "reqLevel", "Cool1", "Cost1", "Area1", "Rng1", "Cool2", "Cost2", "Area2",
            "Rng2", "Cool3", "Cost3", "Area3", "Rng3", "Cool4", "Cost4", "Area4", "Rng4",
        ],
        TableKind::ItemData => &[
            "Level",
            "uses",
            "goldcost",
            "lumbercost",
            "HP",
            // `armor` holds the armour *material* (`Wood`), not a number.
            "stockMax",
            "stockRegen",
            "stockStart",
            "prio",
            "idealPrio",
        ],
        TableKind::UpgradeData => &[
            "maxlevel", "goldbase", "goldmod", "lumberbase", "lumbermod", "timebase", "timemod",
            "base1", "mod1", "base2", "mod2", "base3", "mod3", "base4", "mod4",
        ],
        TableKind::DestructableData => &[
            "HP", "numVar", "radius", "fogRadius", "fogVis", "MMRed", "MMGreen", "MMBlue",
        ],
        TableKind::Doodads => &[
            "numVar", "defScale", "minScale", "maxScale", "MMRed", "MMGreen", "MMBlue",
        ],
    }
}

/// Run every check.
pub fn validate(data: &GameData) -> Report {
    let mut report = Report {
        root: data.root().to_string(),
        ..Report::default()
    };

    // ---- table shape ----------------------------------------------------
    for table in data.tables() {
        let kind = table.kind();
        let mut empty_ids = 0usize;
        let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut duplicates = 0usize;
        for id in table.ids() {
            if id.is_empty() {
                empty_ids += 1;
                continue;
            }
            if id.len() != 4 {
                report.issues.push(Issue::new(
                    Severity::Error,
                    "table.id-length",
                    kind.name(),
                    id.clone(),
                    format!("identifier has {} bytes, expected 4", id.len()),
                ));
            }
            *seen.entry(id.as_str()).or_insert(0) += 1;
        }
        for (id, count) in seen {
            if count > 1 {
                duplicates += count - 1;
                report.issues.push(Issue::new(
                    Severity::Error,
                    "table.duplicate-id",
                    kind.name(),
                    id.to_string(),
                    format!("{count} rows share this identifier"),
                ));
            }
        }
        if empty_ids > 0 {
            report.issues.push(Issue::new(
                Severity::Info,
                "table.spacer-rows",
                kind.name(),
                "",
                format!("{empty_ids} rows have an empty identifier (spacer rows)"),
            ));
        }
        report.tables.push(TableStat {
            kind,
            rows: table.len(),
            columns: table.columns().len(),
            empty_ids,
            duplicate_ids: duplicates,
            shadow_columns: table.sheet().shadow_columns().len(),
            overlay_cells: table.sheet().overlay_cells(),
        });

        // ---- numeric columns -------------------------------------------
        for column in numeric_columns(kind) {
            if !table.has_column(column) {
                continue;
            }
            for id in table.ids() {
                if id.is_empty() {
                    continue;
                }
                match table.f32_strict(id, column) {
                    Ok(_) => {}
                    Err(error) => {
                        let artifact = table.cell(id, column).and_then(|cell| cell.artifact());
                        let garbage = table
                            .cell(id, column)
                            .map(|cell| cell.has_trailing_garbage())
                            .unwrap_or(false);
                        // Known data artifacts (Excel errors, FourCC markers) are
                        // warnings: the engine reads them as 0 and the row stays
                        // usable. Anything else is a real defect.
                        let (severity, code) = match (artifact, garbage) {
                            (Some(wc3_formats::gamedata::Artifact::Spreadsheet), _)
                            | (Some(wc3_formats::gamedata::Artifact::Identifier), _) => {
                                (Severity::Warning, "column.artifact")
                            }
                            (None, true) => (Severity::Warning, "column.trailing-garbage"),
                            (None, false) => (Severity::Error, "column.bad-value"),
                        };
                        report.issues.push(Issue::new(
                            severity,
                            code,
                            kind.name(),
                            id.clone(),
                            format!("{column}: {error}"),
                        ));
                    }
                }
            }
        }
    }

    // ---- joins between the unit sheets ---------------------------------
    check_join(
        data,
        &mut report,
        TableKind::UnitData,
        TableKind::UnitBalance,
        Severity::Error,
        "join.unit-balance",
    );
    check_join(
        data,
        &mut report,
        TableKind::UnitData,
        TableKind::UnitUi,
        Severity::Error,
        "join.unit-ui",
    );
    check_join(
        data,
        &mut report,
        TableKind::UnitData,
        TableKind::UnitWeapons,
        Severity::Info,
        "join.unit-weapons",
    );
    check_join(
        data,
        &mut report,
        TableKind::UnitData,
        TableKind::UnitAbilities,
        Severity::Info,
        "join.unit-abilities",
    );

    // ---- references ----------------------------------------------------
    check_references(data, &mut report);

    // ---- ability levels -------------------------------------------------
    if let Some(abilities) = data.table(TableKind::AbilityData) {
        let mut without_levels = 0usize;
        for id in abilities.ids() {
            if id.is_empty() {
                continue;
            }
            let Some(ability) = data.ability(id) else {
                continue;
            };
            if ability.max_level() == 0 {
                without_levels += 1;
            }
        }
        if without_levels > 0 {
            report.issues.push(Issue::new(
                Severity::Warning,
                "ability.no-level-columns",
                "AbilityData",
                "",
                format!("{without_levels} abilities have no level columns"),
            ));
        }
    }

    // ---- weapon damage invariants ---------------------------------------
    check_weapon_damage(data, &mut report);

    // ---- archive paths --------------------------------------------------
    if let Some(catalog) = data.catalog() {
        check_paths(data, catalog, &mut report);
    }

    // ---- object data ----------------------------------------------------
    for (kind, base, custom, _files) in data.object_data().counts() {
        report.object_data.push((kind, base, custom));
    }
    for merge in data.merges() {
        for (sheet, field) in &merge.report.unknown_field {
            report.issues.push(Issue::new(
                Severity::Warning,
                "objectdata.unknown-field",
                sheet.clone(),
                field.clone(),
                format!("{}: field id has no metadata row", merge.source),
            ));
        }
        for (sheet, field) in &merge.report.missing_sheet {
            report.issues.push(Issue::new(
                Severity::Warning,
                "objectdata.missing-sheet",
                sheet.clone(),
                field.clone(),
                format!("{}: target sheet not loaded", merge.source),
            ));
        }
        for (sheet, id) in &merge.report.prototype_missing {
            report.issues.push(Issue::new(
                Severity::Info,
                "objectdata.no-prototype-row",
                sheet.clone(),
                id.clone(),
                format!("{}: prototype row absent from this sheet", merge.source),
            ));
        }
        report.merged_cells += merge.report.applied + merge.report.shadow + merge.report.profile;
    }

    report
}

fn check_join(
    data: &GameData,
    report: &mut Report,
    left: TableKind,
    right: TableKind,
    severity: Severity,
    code: &'static str,
) {
    let (Some(left_table), Some(right_table)) = (data.table(left), data.table(right)) else {
        return;
    };
    let mut missing = 0usize;
    let mut samples = Vec::new();
    for id in left_table.ids() {
        if id.is_empty() {
            continue;
        }
        if !right_table.contains(id) {
            missing += 1;
            if samples.len() < 8 {
                samples.push(id.clone());
            }
        }
    }
    if missing > 0 {
        report.issues.push(Issue::new(
            severity,
            code,
            right.name(),
            "",
            format!(
                "{missing} of {} rows in {} have no row here (e.g. {})",
                left_table.len(),
                left.name(),
                samples.join(", ")
            ),
        ));
    }
}

fn check_references(data: &GameData, report: &mut Report) {
    let Some(abilities) = data.table(TableKind::AbilityData) else {
        return;
    };
    let sentinels = ["_", "-", ""];

    // Unit ability lists.
    if let Some(units) = data.table(TableKind::UnitAbilities) {
        for id in units.ids() {
            if id.is_empty() {
                continue;
            }
            for column in ["abilList", "heroAbilList"] {
                for reference in units.list(id, column) {
                    if sentinels.contains(&reference) || abilities.contains(reference) {
                        continue;
                    }
                    report.issues.push(Issue::new(
                        Severity::Warning,
                        "ref.ability-missing",
                        "UnitAbilities",
                        id.clone(),
                        format!("{column} references `{reference}` which is absent from AbilityData"),
                    ));
                }
            }
        }
    }

    // Item ability lists.
    if let Some(items) = data.table(TableKind::ItemData) {
        for id in items.ids() {
            if id.is_empty() {
                continue;
            }
            for reference in items.list(id, "abilList") {
                if sentinels.contains(&reference) || abilities.contains(reference) {
                    continue;
                }
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "ref.ability-missing",
                    "ItemData",
                    id.clone(),
                    format!("abilList references `{reference}` which is absent from AbilityData"),
                ));
            }
        }
    }

    // Unit upgrade lists.
    if let (Some(units), Some(upgrades)) = (
        data.table(TableKind::UnitBalance),
        data.table(TableKind::UpgradeData),
    ) {
        for id in units.ids() {
            if id.is_empty() {
                continue;
            }
            for reference in units.list(id, "upgrades") {
                if sentinels.contains(&reference) || upgrades.contains(reference) {
                    continue;
                }
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "ref.upgrade-missing",
                    "UnitBalance",
                    id.clone(),
                    format!("upgrades references `{reference}` which is absent from UpgradeData"),
                ));
            }
        }
    }

    // Upgrade effect codes against the shipped dictionary (TFT only).
    if let (Some(upgrades), Some(meta)) = (
        data.table(TableKind::UpgradeData),
        data.meta_table(MetaKind::UpgradeEffect),
    ) {
        for id in upgrades.ids() {
            if id.is_empty() {
                continue;
            }
            for slot in 1..=4 {
                let Some(code) = upgrades.text(id, &format!("effect{slot}")) else {
                    continue;
                };
                if sentinels.contains(&code) {
                    continue;
                }
                if meta.get(code).is_none() {
                    report.issues.push(Issue::new(
                        Severity::Info,
                        "ref.effect-code-unknown",
                        "UpgradeData",
                        id.clone(),
                        format!("effect{slot} uses `{code}` which UpgradeEffectMetaData does not describe"),
                    ));
                }
            }
        }
    }
}

/// The shipped sheets carry precomputed `mindmg`/`avgdmg`/`maxdmg`; they must
/// agree with the dice formula `dmgplus + Ndice*(N+1)/2`, which proves both the
/// formula and the cell reads.
fn check_weapon_damage(data: &GameData, report: &mut Report) {
    let Some(weapons) = data.table(TableKind::UnitWeapons) else {
        return;
    };
    for id in weapons.ids() {
        if id.is_empty() {
            continue;
        }
        let Some(unit) = data.unit(id) else {
            continue;
        };
        for slot in [crate::units::WeaponSlot::Primary, crate::units::WeaponSlot::Secondary] {
            let Some(weapon) = unit.weapon(slot) else {
                continue;
            };
            let Ok(average) = weapon.average_damage() else {
                continue;
            };
            let Ok(maximum) = weapon.max_damage() else {
                continue;
            };
            let suffix = slot.suffix();
            let stored_avg = weapons.f32(id, &format!("avgdmg{suffix}")).ok().flatten();
            let stored_max = weapons.f32(id, &format!("maxdmg{suffix}")).ok().flatten();
            if let (Some(average), Some(stored)) = (average, stored_avg) {
                if (average - stored).abs() > 0.01 {
                    report.issues.push(Issue::new(
                        Severity::Warning,
                        "weapon.avg-mismatch",
                        "UnitWeapons",
                        id.clone(),
                        format!("avgdmg{suffix} = {stored}, dice formula gives {average}"),
                    ));
                }
            }
            if let (Some(maximum), Some(stored)) = (maximum, stored_max) {
                if (maximum as f32 - stored).abs() > 0.01 {
                    report.issues.push(Issue::new(
                        Severity::Warning,
                        "weapon.max-mismatch",
                        "UnitWeapons",
                        id.clone(),
                        format!("maxdmg{suffix} = {stored}, dice formula gives {maximum}"),
                    ));
                }
            }
        }
    }
}

fn check_paths(data: &GameData, catalog: &Listfile, report: &mut Report) {
    report.catalog = Some(CatalogStat {
        origin: catalog.origin().to_string(),
        entries: catalog.entries(),
        paths: catalog.len(),
        unit_models: catalog.count("Units\\", ".mdx"),
        building_models: catalog.count("Buildings\\", ".mdx"),
        command_button_icons: catalog.count("ReplaceableTextures\\CommandButtons\\", ".blp"),
    });

    let mut unit_models = PathStat::new("unit-model");
    let mut unit_portraits = PathStat::new("unit-portrait");
    let mut unit_icons = PathStat::new("unit-icon");
    let mut building_models = PathStat::new("building-model");
    let mut ability_icons = PathStat::new("ability-icon");
    let mut item_icons = PathStat::new("item-icon");
    let mut item_models = PathStat::new("item-model");
    let mut destructable_models = PathStat::new("destructable-model");
    let mut doodad_models = PathStat::new("doodad-model");

    for unit in data.units() {
        let id = unit.id_str();
        let building = unit.is_building().unwrap_or(false);
        if let Some(model) = unit.model_file() {
            let resolved = catalog.resolve_model(&model).is_some();
            if building {
                building_models.record(&id, &model, resolved);
            } else {
                unit_models.record(&id, &model, resolved);
            }
            if !resolved {
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "path.model-missing",
                    "unitUI",
                    id.clone(),
                    format!("model `{model}` is not in the listfile"),
                ));
            }
        }
        if let Some(portrait) = unit.portrait_file() {
            let resolved = catalog.resolve_model(&portrait).is_some();
            unit_portraits.record(&id, &portrait, resolved);
        }
        if let Some(icon) = unit.icon() {
            if !crate::art::is_icon_path(&icon) {
                unit_icons.record_identifier();
                continue;
            }
            let resolved = catalog.resolve_file(&icon).is_some();
            unit_icons.record(&id, &icon, resolved);
            if !resolved {
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "path.icon-missing",
                    "Profile",
                    id.clone(),
                    format!("icon `{icon}` is not in the listfile"),
                ));
            }
        }
    }

    for ability in data.abilities() {
        if let Some(icon) = ability.icon() {
            if !crate::art::is_icon_path(&icon) {
                ability_icons.record_identifier();
                continue;
            }
            let resolved = catalog.resolve_file(&icon).is_some();
            ability_icons.record(&ability.id_str(), &icon, resolved);
            if !resolved {
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "path.icon-missing",
                    "Profile",
                    ability.id_str(),
                    format!("icon `{icon}` is not in the listfile"),
                ));
            }
        }
    }

    for item in data.items() {
        if let Some(icon) = item.icon() {
            if !crate::art::is_icon_path(&icon) {
                item_icons.record_identifier();
                continue;
            }
            let resolved = catalog.resolve_file(&icon).is_some();
            item_icons.record(&item.id_str(), &icon, resolved);
            if !resolved {
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "path.icon-missing",
                    "Profile",
                    item.id_str(),
                    format!("icon `{icon}` is not in the listfile"),
                ));
            }
        }
        if let Some(model) = item.model_file() {
            let resolved = catalog.resolve_model(&model).is_some();
            item_models.record(&item.id_str(), &model, resolved);
            if !resolved {
                report.issues.push(Issue::new(
                    Severity::Warning,
                    "path.model-missing",
                    "ItemData",
                    item.id_str(),
                    format!("model `{model}` is not in the listfile"),
                ));
            }
        }
    }

    for object in data.destructables() {
        let variants = object.model_variants();
        if variants.is_empty() {
            continue;
        }
        let resolved = object.model_exists().unwrap_or(false);
        destructable_models.record(&object.id_str(), &variants[0], resolved);
        if !resolved {
            report.issues.push(Issue::new(
                Severity::Warning,
                "path.model-missing",
                "DestructableData",
                object.id_str(),
                format!("none of {} model variations are in the listfile", variants.len()),
            ));
        }
    }

    for object in data.doodads() {
        let variants = object.model_variants();
        if variants.is_empty() {
            continue;
        }
        let resolved = object.model_exists().unwrap_or(false);
        doodad_models.record(&object.id_str(), &variants[0], resolved);
        if !resolved {
            report.issues.push(Issue::new(
                Severity::Warning,
                "path.model-missing",
                "Doodads",
                object.id_str(),
                format!("none of {} model variations are in the listfile", variants.len()),
            ));
        }
    }

    report.paths.extend([
        unit_models,
        building_models,
        unit_portraits,
        unit_icons,
        ability_icons,
        item_icons,
        item_models,
        destructable_models,
        doodad_models,
    ]);
}

impl Report {
    /// Human readable summary (one line per table/path class plus issue counts).
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("root: {}", self.root));
        for stat in &self.tables {
            lines.push(format!(
                "table {:<16} rows={:<5} cols={:<3} empty_ids={} dup_ids={} overlay_cells={}",
                stat.kind.name(),
                stat.rows,
                stat.columns,
                stat.empty_ids,
                stat.duplicate_ids,
                stat.overlay_cells
            ));
        }
        for stat in &self.paths {
            lines.push(format!(
                "paths {:<20} checked={:<5} resolved={:<5} missing={:<4} aliases={}",
                stat.class, stat.checked, stat.resolved, stat.missing, stat.identifiers
            ));
        }
        if let Some(catalog) = &self.catalog {
            lines.push(format!(
                "catalog {} entries={} paths={} unit_models={} building_models={} command_icons={}",
                catalog.origin,
                catalog.entries,
                catalog.paths,
                catalog.unit_models,
                catalog.building_models,
                catalog.command_button_icons
            ));
        }
        let errors = self.error_count();
        let warnings = self.warning_count();
        lines.push(format!(
            "issues errors={errors} warnings={warnings} total={}",
            self.issues.len()
        ));
        lines
    }
}

/// Convenience: validate and return only errors, for tests.
pub fn errors(data: &GameData) -> Vec<Issue> {
    validate(data).errors().cloned().collect()
}

/// Unused-table helper for tests: rows in `kind`.
pub fn rows(data: &GameData, kind: TableKind) -> usize {
    data.table(kind).map(Table::len).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    #[test]
    fn detects_duplicates_bad_numbers_and_missing_refs() {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\UnitData.slk",
            b"ID;PWXL;N;E\nB;Y3;X2;D0\nC;Y1;X1;K\"unitID\"\nC;X2;K\"race\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"human\"\nC;Y3;X1;K\"hfoo\"\nC;X2;K\"orc\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\UnitBalance.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitBalanceID\"\nC;X2;K\"HP\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"abc\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\AbilityData.slk",
            b"ID;PWXL;N;E\nB;Y2;X1;D0\nC;Y1;X1;K\"alias\"\nC;Y2;X1;K\"Adef\"\n".to_vec(),
        );
        source.insert(
            "Units\\UnitAbilities.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitAbilID\"\nC;X2;K\"abilList\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"A000\"\n"
                .to_vec(),
        );
        let data = GameData::load(&source).expect("load");
        let report = data.validate();
        assert!(report.error_count() >= 2, "{}", report.summary_lines().join("\n"));
        let error_codes: Vec<&str> = report.errors().map(|issue| issue.code).collect();
        assert!(error_codes.contains(&"table.duplicate-id"));
        assert!(error_codes.contains(&"column.bad-value"));
        // FourCC markers in numeric columns are data artifacts, not parser
        // failures: the engine reads them as 0.
        let artifact = wc3_formats::gamedata::Cell::new("MBUR").artifact();
        assert_eq!(
            artifact,
            Some(wc3_formats::gamedata::Artifact::Identifier)
        );
        assert_eq!(
            wc3_formats::gamedata::Cell::new("#VALUE!").artifact(),
            Some(wc3_formats::gamedata::Artifact::Spreadsheet)
        );
        // Dangling object references are warnings: the shipped RoC tables point
        // at abilities only TFT defines, and the game tolerates that.
        let warning_codes: Vec<&str> = report.warnings().map(|issue| issue.code).collect();
        assert!(warning_codes.contains(&"ref.ability-missing"));
    }
}
