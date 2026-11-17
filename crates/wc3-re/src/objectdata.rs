//! Object database: SLK tables + `*Func.txt` + `*Strings.txt` + map overrides.
//!
//! The classic loader composes object data from several layers:
//!
//! 1. `Units\UnitData.slk`, `UnitBalance.slk`, `UnitWeapons.slk`,
//!    `UnitAbilities.slk`, `unitUI.slk` (columns keyed by `unitID` /
//!    `unitBalanceID` / `unitWeapID` / `unitAbilID` / `unitUIID`).
//! 2. `Units\AbilityData.slk` (`alias`), `ItemData.slk` (`itemID`),
//!    `UpgradeData.slk` (`upgradeid`), `Doodads\Doodads.slk` (`doodID`),
//!    `Units\DestructableData.slk` (row key in the first column).
//! 3. Text layer: every `Units\*UnitFunc.txt`, `*AbilityFunc.txt`,
//!    `*UpgradeFunc.txt`, `ItemFunc.txt`, `CommonAbilityFunc.txt`,
//!    `CampaignUnitFunc.txt` and the matching `*Strings.txt`; sections are the
//!    object id and entries are the editor field names (`Art`, `Name`, `Tip`,
//!    `Ubertip`, `Buttonpos`, …).
//! 4. `Units\UnitMetaData.slk` maps a four-character modification id (the ids
//!    stored in `war3map.w3u/.w3a/.w3t/…`) to `(slk table, field name)`, e.g.
//!    `uhpm -> (UnitBalance, HP)`, `unam -> (UnitBalance? no: text)`. This is
//!    what makes map object-data editing (and the JASS `BlzSetUnit*` family)
//!    resolvable by id.
//!
//! Map overrides (`war3map.w3u` etc.) replace `old_id` with `new_id` and apply
//! typed modifications; [`ObjectDb::apply_map_objects`] performs that merge and
//! keeps both the base id and the modified values.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::mapobjects::{MapObjectData, ModValue};
use wc3_formats::gamedata::{MetaKind, MetaTable, Sheet as TypedSheet};

use crate::error::Result;
use crate::storm::StormFs;
use crate::txt::TextTable;

/// Which table a sheet is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectKind {
    Unit,
    Ability,
    Item,
    Destructable,
    Upgrade,
    Doodad,
    Buff,
    Misc,
    Unknown,
}

impl Default for ObjectKind {
    fn default() -> Self {
        Self::Unknown
    }
}

impl ObjectKind {
    fn from_sheet(name: &str) -> Self {
        match name {
            "UnitData" | "UnitBalance" | "UnitWeapons" | "UnitAbilities" | "unitUI" => {
                Self::Unit
            }
            "AbilityData" => Self::Ability,
            "ItemData" => Self::Item,
            "DestructableData" => Self::Destructable,
            "UpgradeData" => Self::Upgrade,
            "Doodads" => Self::Doodad,
            "BuffData" => Self::Buff,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Ability => "ability",
            Self::Item => "item",
            Self::Destructable => "destructable",
            Self::Upgrade => "upgrade",
            Self::Doodad => "doodad",
            Self::Buff => "buff",
            Self::Misc => "misc",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FieldMeta {
    pub id: String,
    pub field: String,
    pub slk: String,
    pub index: String,
    pub value_type: String,
    pub display: String,
    pub can_be_empty: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ObjectRecord {
    pub id: String,
    pub kind: ObjectKind,
    /// Text-layer fields (`Art`, `Name`, `Tip`, …), last writer wins.
    pub fields: BTreeMap<String, String>,
    /// Per-SLK fields as `table.column` (collision-free).
    pub slk_fields: BTreeMap<String, String>,
    /// Base id for a map-created object (`new_id` → `old_id`).
    pub base_id: Option<String>,
    pub sources: Vec<String>,
}

impl ObjectRecord {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn slk_field(&self, table: &str, column: &str) -> Option<&str> {
        // Archive listfiles are case-insensitive and spell sheet names in
        // lowercase (`doodads`, `unitdata`); the accessor must not depend on the
        // caller's casing.
        let table = table
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(table)
            .to_ascii_lowercase();
        let key = format!("{table}.{}", column.to_ascii_lowercase());
        self.slk_fields.get(&key).map(String::as_str)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObjectDb {
    /// Typed canonical SLK views (`wc3-formats::gamedata::Sheet`).
    pub sheets: BTreeMap<String, TypedSheet>,
    pub objects: BTreeMap<String, ObjectRecord>,
    pub metadata: BTreeMap<String, FieldMeta>,
    pub loaded_files: Vec<(String, usize)>,
    /// Canonical `UnitMetaData.slk` parser result; projected into `metadata`
    /// for the historical `FieldMeta` API.
    pub metadata_table: Option<MetaTable>,
    /// Metadata tables for every object kind (`AbilityMetaData`,
    /// `UpgradeMetaData`, `DestructableMetaData`, `DoodadMetaData`, …). Field ids
    /// are only unique per kind (`Hbz1` is a Blizzard field, `uhpm` a unit
    /// field), so the merge resolves through the table of the object's kind.
    pub metas: BTreeMap<MetaKind, MetaTable>,
    /// Parsed profile text layers, retained so the typed database can be built
    /// from these values without a second read.
    pub texts: Vec<TextTable>,
    /// Text overrides written by `apply_map_objects` (`unam`, `utip`, `aart`, …),
    /// shared with the typed database.
    pub profile_overlay: wc3_formats::gamedata::ProfileOverlay,
}

impl ObjectDb {
    /// Load every object table from an extracted install directory.
    pub fn load_dir(root: &Path) -> Result<Self> {
        let mut db = Self::default();
        let units = root.join("Units");
        for entry in fs::read_dir(&units)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "slk") {
                let bytes = fs::read(&path)?;
                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                db.add_sheet(&*name, &bytes)?;
            }
        }
        let doodads = root.join("Doodads/Doodads.slk");
        if doodads.is_file() {
            db.add_sheet("Doodads", &fs::read(doodads)?)?;
        }
        for entry in fs::read_dir(&units)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "txt") {
                let bytes = fs::read(&path)?;
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                db.add_text(name.as_str(), &bytes)?;
            }
        }
        db.build_records();
        db.load_metadata();
        Ok(db)
    }

    /// Load from an open Storm filesystem (archives or loose tree). The
    /// listfile is used to find every `Units/*.slk|txt` and
    /// `Doodads/Doodads.slk`.
    pub fn load_storm(fs: &StormFs) -> Result<Self> {
        let mut db = Self::default();
        let names = fs.listfile();
        for name in &names {
            let lower = name.to_ascii_lowercase();
            let in_units = lower.starts_with("units\\");
            let doodads = lower == "doodads\\doodads.slk";
            if !(in_units || doodads) {
                continue;
            }
            if lower.ends_with(".slk") {
                // Archive names use `\` separators; `Path` on Unix does not split
                // them, so take the last component explicitly before the stem.
                let file_name = name.rsplit(['\\', '/']).next().unwrap_or(name);
                let stem = Path::new(file_name)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if let Ok(bytes) = fs.read(name) {
                    db.add_sheet(stem.as_str(), &bytes)?;
                }
            } else if lower.ends_with(".txt") {
                if let Ok(bytes) = fs.read(name) {
                    db.add_text(name.as_str(), &bytes)?;
                }
            }
        }
        db.build_records();
        db.load_metadata();
        Ok(db)
    }

    fn add_sheet(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        const META_SHEETS: [(MetaKind, &str); 9] = [
            (MetaKind::Unit, "UnitMetaData"),
            (MetaKind::Ability, "AbilityMetaData"),
            (MetaKind::Upgrade, "UpgradeMetaData"),
            (MetaKind::Destructable, "DestructableMetaData"),
            (MetaKind::Doodad, "DoodadMetaData"),
            (MetaKind::Misc, "MiscMetaData"),
            (MetaKind::Skin, "SkinMetaData"),
            (MetaKind::UpgradeEffect, "UpgradeEffectMetaData"),
            (MetaKind::Buff, "AbilityBuffMetaData"),
        ];
        let sheet = TypedSheet::parse(name, bytes)
            .map_err(|err| crate::error::ReError::Parse(format!("SLK {name}: {err}")))?;
        for (kind, sheet_name) in META_SHEETS {
            if !name.eq_ignore_ascii_case(sheet_name) {
                continue;
            }
            let table = MetaTable::parse(kind, bytes)
                .map_err(|err| crate::error::ReError::Parse(format!("MetaTable {name}: {err}")))?;
            if kind == MetaKind::Unit {
                self.metadata_table = Some(table.clone());
            }
            self.metas.insert(kind, table);
        }
        self.loaded_files
            .push((format!("{name}.slk"), bytes.len()));
        self.sheets.insert(name.to_string(), sheet);
        Ok(())
    }

    fn add_text(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let table = TextTable::parse(name, bytes)?;
        self.loaded_files.push((name.to_string(), bytes.len()));
        for section in &table.sections {
            let record = self
                .objects
                .entry(section.name.clone())
                .or_insert_with(|| ObjectRecord {
                    id: section.name.clone(),
                    kind: infer_kind(name),
                    ..ObjectRecord::default()
                });
            if record.kind == ObjectKind::Unknown {
                record.kind = infer_kind(name);
            }
            if !record.sources.iter().any(|source| source == name) {
                record.sources.push(name.to_string());
            }
            for (key, value) in &section.entries {
                record.fields.insert(key.clone(), value.clone());
            }
        }
        self.texts.push(table);
        Ok(())
    }

    fn build_records(&mut self) {
        let sheet_names: Vec<String> = self.sheets.keys().cloned().collect();
        for sheet_name in sheet_names {
            if is_metadata_sheet(&sheet_name) {
                continue;
            }
            let Some(sheet) = self.sheets.get(&sheet_name).cloned() else {
                continue;
            };
            let kind = ObjectKind::from_sheet(&sheet_name);
            let key_column = match sheet_name.as_str() {
                "UnitData" => "unitID",
                "UnitBalance" => "unitBalanceID",
                "UnitWeapons" => "unitWeapID",
                "UnitAbilities" => "unitAbilID",
                "unitUI" => "unitUIID",
                "AbilityData" => "alias",
                "ItemData" => "itemID",
                "UpgradeData" => "upgradeid",
                _ => "",
            };
            let key_index = if key_column.is_empty() {
                None
            } else {
                sheet.column_index(key_column)
            };
            for row in 0..sheet.len() {
                let id = key_index
                    .and_then(|index| sheet.raw_cell(row, index))
                    .map(|cell| cell.raw().to_string())
                    .or_else(|| {
                        sheet
                            .row(row)
                            .and_then(|values| values.first())
                            .cloned()
                    })
                    .unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let record = self.objects.entry(id.clone()).or_insert_with(|| ObjectRecord {
                    id: id.clone(),
                    kind,
                    ..ObjectRecord::default()
                });
                if record.kind == ObjectKind::Unknown {
                    record.kind = kind;
                }
                if !record
                    .sources
                    .iter()
                    .any(|source| source == &format!("{sheet_name}.slk"))
                {
                    record.sources.push(format!("{sheet_name}.slk"));
                }
                if let Some(values) = sheet.row(row) {
                    for (index, column) in sheet.columns().iter().enumerate() {
                        if key_index == Some(index) {
                            continue;
                        }
                        let Some(value) = values.get(index) else {
                            continue;
                        };
                        if value.is_empty() {
                            continue;
                        }
                        record.slk_fields.insert(
                            format!(
                                "{}.{}",
                                sheet_name.to_ascii_lowercase(),
                                column.to_ascii_lowercase()
                            ),
                            value.clone(),
                        );
                    }
                }
            }
        }
    }

    fn load_metadata(&mut self) {
        for table in self.metas.values() {
            for field in table.fields() {
                let id = field.id();
                if id.is_empty() {
                    continue;
                }
                self.metadata.entry(id.to_string()).or_insert_with(|| FieldMeta {
                    id: id.to_string(),
                    field: field.field().to_string(),
                    slk: field.slk().to_string(),
                    index: field.index().to_string(),
                    value_type: field.type_name().to_string(),
                    display: field.display_name().to_string(),
                    can_be_empty: field.raw_column("canBeEmpty") == Some("1"),
                });
            }
        }
    }

    pub fn record(&self, id: &str) -> Option<&ObjectRecord> {
        self.objects.get(id)
    }

    /// Resolve a four-character modification id (as stored in map object data
    /// and used by `BlzSet*` natives) through `UnitMetaData.slk`.
    pub fn field_by_id(&self, object_id: &str, modification: &[u8; 4]) -> Option<String> {
        let id = crate::fourcc_str(modification);
        let record = self.objects.get(object_id)?;
        if let Some(meta) = self.metadata.get(&id) {
            if let Some(value) = record.slk_field(&meta.slk, &meta.field) {
                return Some(value.to_string());
            }
            if let Some(value) = record.field(&meta.field) {
                return Some(value.to_string());
            }
            // A map override may have set the text field from `new_id`.
            if let Some(value) = record.field(&format!("{id}")) {
                return Some(value.to_string());
            }
        }
        record.field(&id).map(|value| value.to_string())
    }

    /// Apply `war3map.w3u/.w3a/.w3t/.w3b/.w3h/.w3q` modifications.
    ///
    /// The merge is canonical: `wc3_formats::gamedata::merge_overlay` writes each
    /// change into the sheet column named by its metadata (including the
    /// level/data-pointer suffix of ability/upgrade/doodad changes and the
    /// `ItemData` redirection of `isst`), or into the text layer for `Profile`
    /// fields. Records are refreshed for every touched id, so `slk_field()` and
    /// `field()` agree with the typed database.
    pub fn apply_map_objects(&mut self, objects: &MapObjectData) -> usize {
        let file = objects.to_typed();
        let kind = file.kind;
        let meta = self.meta_for_typed(kind).cloned();
        let mut profile = self.profile_overlay.clone();
        let report;
        {
            let mut targets: Vec<wc3_formats::gamedata::MergeTarget<'_>> = Vec::new();
            for (sheet_name, sheet) in self.sheets.iter_mut() {
                if is_metadata_sheet(sheet_name) {
                    continue;
                }
                // `DoodadMetaData.slk` names the doodad sheet `DoodadData`.
                let target_name: &str = if sheet_name == "Doodads" {
                    "DoodadData"
                } else {
                    sheet_name.as_str()
                };
                targets.push(wc3_formats::gamedata::MergeTarget {
                    name: target_name,
                    sheet,
                    create_rows: is_primary_sheet(kind, sheet_name),
                });
            }
            report = wc3_formats::gamedata::merge_overlay(
                kind,
                &file,
                meta.as_ref(),
                &mut targets,
                &mut profile,
            );
        }
        self.profile_overlay = profile;

        let mut touched: BTreeSet<String> = BTreeSet::new();
        for entry in file.entries() {
            let target = entry.target_id().as_str().into_owned();
            if entry.is_custom() && !entry.old_id.is_zero() {
                let old_id = entry.old_id.as_str().into_owned();
                let mut base_kind = object_kind_from_typed(kind);
                let mut base_fields = BTreeMap::new();
                if let Some(base) = self.objects.get(&old_id) {
                    base_kind = base.kind;
                    base_fields = base.fields.clone();
                }
                let record = self.objects.entry(target.clone()).or_insert_with(|| ObjectRecord {
                    id: target.clone(),
                    kind: base_kind,
                    fields: base_fields,
                    ..ObjectRecord::default()
                });
                record.base_id = Some(old_id);
                if !record
                    .sources
                    .iter()
                    .any(|source| source == "war3map object data")
                {
                    record.sources.push("war3map object data".to_string());
                }
            }
            touched.insert(target);
        }

        for (id, section) in self.profile_overlay.sections() {
            if let Some(record) = self.objects.get_mut(id) {
                for (key, components) in section {
                    // ProfileOverlay keys are lowercased; replace the case-
                    // insensitive existing key (e.g. `Name`) instead of adding
                    // a second lowercase entry that `field()` may shadow.
                    record
                        .fields
                        .retain(|existing, _| !existing.eq_ignore_ascii_case(key));
                    record.fields.insert(key.clone(), components.join(","));
                }
            }
        }
        for id in &touched {
            self.refresh_record(id);
        }
        report.applied + report.shadow + report.profile
    }

    /// Metadata table for a typed object kind (items fall back to the unit
    /// sheet, which is where retail keeps item fields).
    pub fn meta_for_typed(&self, kind: wc3_formats::gamedata::ObjectKind) -> Option<&MetaTable> {
        use wc3_formats::gamedata::ObjectKind as Typed;
        let meta_kind = match kind {
            Typed::Unit => MetaKind::Unit,
            Typed::Ability => MetaKind::Ability,
            Typed::Item => MetaKind::Item,
            Typed::Upgrade => MetaKind::Upgrade,
            Typed::Buff => MetaKind::Buff,
            Typed::Destructable => MetaKind::Destructable,
            Typed::Doodad => MetaKind::Doodad,
        };
        self.metas.get(&meta_kind).or_else(|| {
            meta_kind
                .item_fallback()
                .and_then(|fallback| self.metas.get(&fallback))
        })
    }

    /// Rebuild the `table.column` projection of one object from the (possibly
    /// overlaid) canonical sheets.
    fn refresh_record(&mut self, id: &str) {
        let mut slk_fields: BTreeMap<String, String> = BTreeMap::new();
        for (sheet_name, sheet) in &self.sheets {
            let key_column = key_column(sheet_name);
            if key_column.is_empty() {
                continue;
            }
            if sheet.id_index(id).is_none() {
                continue;
            }
            for column in sheet.columns() {
                if column.eq_ignore_ascii_case(key_column) {
                    continue;
                }
                let Some((cell, _origin)) = sheet.cell_with_overlay(id, column) else {
                    continue;
                };
                let Some(value) = cell.text() else { continue };
                slk_fields.insert(
                    format!(
                        "{}.{}",
                        sheet_name.to_ascii_lowercase(),
                        column.to_ascii_lowercase()
                    ),
                    value.to_string(),
                );
            }
            // Columns created by the overlay (metadata fields whose sheet column
            // does not exist in the shipped table).
            for column in sheet.shadow_columns() {
                let Some((cell, _origin)) = sheet.cell_with_overlay(id, column) else {
                    continue;
                };
                let Some(value) = cell.text() else { continue };
                slk_fields.insert(
                    format!(
                        "{}.{}",
                        sheet_name.to_ascii_lowercase(),
                        column.to_ascii_lowercase()
                    ),
                    value.to_string(),
                );
            }
        }
        if let Some(record) = self.objects.get_mut(id) {
            record.slk_fields = slk_fields;
        }
    }

    /// Parsed profile text layers kept from the install (typed bridge input).
    pub fn texts(&self) -> &[TextTable] {
        &self.texts
    }

    /// Metadata tables per object kind (typed bridge input).
    pub fn metas(&self) -> &BTreeMap<MetaKind, MetaTable> {
        &self.metas
    }

    /// Resolve `TRIGSTR_<n>` in every text field through the map's `.wts`.
    pub fn resolve_texts(&mut self, wts: &crate::wts::Wts) -> usize {
        let mut changed = 0;
        for record in self.objects.values_mut() {
            for value in record.fields.values_mut() {
                if !value.contains("TRIGSTR_") {
                    continue;
                }
                let resolved = wts.resolve(value);
                if resolved != *value {
                    *value = resolved;
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Object counts by kind (units, abilities, items, …).
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for record in self.objects.values() {
            *counts.entry(record.kind.label()).or_insert(0) += 1;
        }
        counts
    }

    pub fn sheet_counts(&self) -> BTreeMap<String, usize> {
        self.sheets
            .iter()
            .map(|(name, sheet)| (name.clone(), sheet.len()))
            .collect()
    }

    /// Every BLP path referenced by an `*art` field.
    pub fn icon_paths(&self) -> Vec<IconRef> {
        let mut out = Vec::new();
        for record in self.objects.values() {
            for (field, value) in &record.fields {
                let lower = field.to_ascii_lowercase();
                if !lower.contains("art") {
                    continue;
                }
                // Upgrade/ability art fields hold a comma-separated level list,
                // and CampaignUnitFunc.txt has the retail typo `Art=Art=...`.
                for raw in value.split(',') {
                    let mut path = raw.trim().trim_start_matches("Art=").trim();
                    while let Some(rest) = path.strip_prefix("Art=") {
                        path = rest.trim();
                    }
                    if !path.to_ascii_lowercase().ends_with(".blp") {
                        continue;
                    }
                    out.push(IconRef {
                        kind: record.kind,
                        object_id: record.id.clone(),
                        field: field.clone(),
                        path: path.to_string(),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path).then(a.object_id.cmp(&b.object_id)));
        out
    }

    pub fn known_ids(&self) -> BTreeSet<&str> {
        self.objects.keys().map(String::as_str).collect()
    }
}

/// Metadata sheets are parsed into `MetaTable`s, never projected as objects.
fn is_metadata_sheet(name: &str) -> bool {
    name.ends_with("MetaData")
}

/// Sheets whose rows are created for a custom (map-authored) object.
fn is_primary_sheet(kind: wc3_formats::gamedata::ObjectKind, sheet: &str) -> bool {
    use wc3_formats::gamedata::ObjectKind as Typed;
    match kind {
        Typed::Unit => matches!(
            sheet,
            "UnitData" | "UnitBalance" | "UnitWeapons" | "UnitAbilities" | "unitUI"
        ),
        Typed::Ability => sheet == "AbilityData",
        Typed::Item => sheet == "ItemData",
        Typed::Upgrade => sheet == "UpgradeData",
        Typed::Destructable => sheet == "DestructableData",
        Typed::Doodad => sheet == "Doodads",
        Typed::Buff => false,
    }
}

/// Key column of an object sheet (`""` when the sheet is not an object table).
fn key_column(sheet: &str) -> &'static str {
    match sheet {
        "UnitData" => "unitID",
        "UnitBalance" => "unitBalanceID",
        "UnitWeapons" => "unitWeapID",
        "UnitAbilities" => "unitAbilID",
        "unitUI" => "unitUIID",
        "AbilityData" => "alias",
        "ItemData" => "itemID",
        "UpgradeData" => "upgradeid",
        _ => "",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IconRef {
    pub kind: ObjectKind,
    pub object_id: String,
    pub field: String,
    pub path: String,
}

fn object_kind_from_typed(kind: wc3_formats::gamedata::ObjectKind) -> ObjectKind {
    use wc3_formats::gamedata::ObjectKind as Typed;
    match kind {
        Typed::Unit => ObjectKind::Unit,
        Typed::Ability => ObjectKind::Ability,
        Typed::Item => ObjectKind::Item,
        Typed::Destructable => ObjectKind::Destructable,
        Typed::Upgrade => ObjectKind::Upgrade,
        Typed::Doodad => ObjectKind::Doodad,
        Typed::Buff => ObjectKind::Buff,
    }
}

fn infer_kind(file: &str) -> ObjectKind {
    let lower = file.to_ascii_lowercase();
    if lower.contains("ability") {
        ObjectKind::Ability
    } else if lower.contains("item") {
        ObjectKind::Item
    } else if lower.contains("upgrade") {
        ObjectKind::Upgrade
    } else if lower.contains("destructable") {
        ObjectKind::Destructable
    } else if lower.contains("doodad") {
        ObjectKind::Doodad
    } else if lower.contains("unit") || lower.contains("campaign") {
        ObjectKind::Unit
    } else {
        ObjectKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
    }

    #[test]
    fn applies_map_objects_through_canonical_merge() {
        let root = root();
        if !root.is_dir() {
            eprintln!("skip: extracted install missing");
            return;
        }
        let mut db = ObjectDb::load_dir(&root).expect("load objects");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes()); // version
        bytes.extend_from_slice(&1u32.to_le_bytes()); // base section
        bytes.extend_from_slice(b"hfoo");
        bytes.extend_from_slice(&[0u8; 4]); // base modification
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(b"uhpm");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&777i32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"unam");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"TestName\0");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // custom section
        let objects = MapObjectData::parse(&bytes).expect("synthetic w3u");
        let applied = db.apply_map_objects(&objects);
        assert_eq!(applied, 2);
        let footman = db.record("hfoo").expect("hfoo");
        assert_eq!(footman.slk_field("UnitBalance", "HP"), Some("777"));
        assert_eq!(footman.field("Name"), Some("TestName"));
    }

    #[test]
    fn loads_real_object_database() {
        let db = ObjectDb::load_dir(&root()).expect("load objects");
        let footman = db.record("hfoo").expect("hfoo");
        assert_eq!(
            footman.field("Art"),
            Some("ReplaceableTextures\\CommandButtons\\BTNFootman.blp")
        );
        assert!(footman.slk_field("UnitBalance", "HP").is_some());
        let meta = db.metadata.get("uhpm").expect("uhpm metadata");
        assert_eq!(meta.slk, "UnitBalance");
        assert_eq!(meta.field, "HP");
        assert!(db.icon_paths().len() > 100);
    }
}
