//! Typed Warcraft III game data.
//!
//! `wc3-data` turns the shipped SLK/TXT/text tables into a typed database:
//!
//! ```text
//! GameData
//! ├─ tables      UnitData / UnitBalance / UnitWeapons / UnitAbilities / unitUI
//! │              AbilityData / ItemData / UpgradeData / DestructableData / Doodads
//! ├─ meta        UnitMetaData / AbilityMetaData / … (field id → column)
//! ├─ localizer   *Strings.txt + *Func.txt + war3map.wts + object-data overrides
//! ├─ enums       UnitEditorData.txt dictionaries (weaponType, attackType, …)
//! ├─ misc        MiscData.txt / MiscUI.txt / WorldEditData.txt / UnitEditorData.txt
//! ├─ catalog     archive listfile (model/icon existence)
//! └─ object_data war3map.w3u/.w3a/.w3t/.w3q/.w3b/.w3d/.w3h overlays
//! ```
//!
//! Cells keep the shipped text (so unknown fields stay reachable): typed readers
//! convert on demand and report a [`DataError::BadCell`] instead of guessing.
//!
//! # Example
//!
//! ```no_run
//! use wc3_data::{source::DirSource, GameDatabase};
//!
//! let source = DirSource::new("analysis/war3_data");
//! let data = GameDatabase::load(&source)?;
//! let footman = data.unit("hfoo").expect("footman");
//! println!("{:?} hp={:?}", footman.name(), footman.hp()?);
//! # Ok::<(), wc3_data::DataError>(())
//! ```

pub mod abilities;
pub mod ability_def;
pub mod art;
pub mod balance;
pub mod buffs;
pub mod coverage;
pub mod defs;
pub mod drops;
pub mod destructables;
pub mod doodads;
pub mod enums;
pub mod error;
pub mod hero;
pub mod items;
pub mod localization;
pub mod menu;
pub mod misc;
pub mod object;
pub mod objectdata;
pub mod shop;
pub mod source;
pub mod table;
pub mod units;
pub mod upgrades;
pub mod validate;

use std::collections::BTreeMap;

use wc3_formats::gamedata::{
    ChangeValue, MergeReport, MergeTarget, MetaKind, MetaTable, ObjectDataFile, ObjectKind,
};

use crate::abilities::AbilityView;
use crate::buffs::BuffStore;
pub use crate::coverage::{
    ClassCoverage, CoverageReport, Field, FieldCoverage, MenuCoverage, ObjectClass, RowCoverage,
    RowFacts,
};
use crate::destructables::DestructableView;
use crate::doodads::DoodadView;
use crate::enums::EditorEnums;
pub use crate::error::{DataError, Result};
pub use crate::ability_def::{AbilityNumbers, AuraNumbers, HeroAbilityInfo, SpellLevel};
pub use crate::hero::{HeroAttributes, HeroDef, HeroRules, HeroSkill, PrimaryAttribute, ReviveCost};
pub use crate::menu::MenuEntry;
pub use crate::object::{ObjectInfo, ObjectStats};
pub use crate::defs::{
    Defs, DestructableDef, DoodadDef, ItemDef, UnitDef, UnparsedRow, UpgradeDef,
};
pub use crate::items::{ItemUsage, ItemUsageInfo};
pub use crate::drops::{DropItem, DropLevelRule, DropTable};
pub use crate::shop::{Requirement, ShopDef, ShopItem};
use crate::items::ItemView;
use crate::localization::Localizer;
use crate::misc::MiscTables;
use crate::objectdata::ObjectDataStore;
use crate::source::{Listfile, Source};
pub use crate::table::{Table, TableKind};
use crate::units::UnitView;
use crate::upgrades::UpgradeView;

/// Profile text files, without the `Strings`/`Func` suffix.
///
/// `Strings` files hold localised text, `Func` files hold functional values
/// (icons, art, sounds). Both are loaded; `Func` wins on duplicate keys because
/// it is loaded last.
const PROFILE_BASES: [&str; 23] = [
    "UnitGlobal",
    "Command",
    "CommonAbility",
    "CampaignUnit",
    "CampaignAbility",
    "CampaignUpgrade",
    "HumanUnit",
    "HumanAbility",
    "HumanUpgrade",
    "OrcUnit",
    "OrcAbility",
    "OrcUpgrade",
    "UndeadUnit",
    "UndeadAbility",
    "UndeadUpgrade",
    "NightElfUnit",
    "NightElfAbility",
    "NightElfUpgrade",
    "NeutralUnit",
    "NeutralAbility",
    "NeutralUpgrade",
    "Item",
    "ItemAbility",
];

/// Metadata sheets loaded when present.
const META_KINDS: [MetaKind; 9] = [
    MetaKind::Unit,
    MetaKind::Ability,
    MetaKind::Upgrade,
    MetaKind::Destructable,
    MetaKind::Doodad,
    MetaKind::Misc,
    MetaKind::Skin,
    MetaKind::UpgradeEffect,
    MetaKind::Buff,
];

/// Backwards-compatible alias: `GameDatabase` is the canonical name in
/// `analysis/tech-lead/BASE.md`.
pub type GameData = GameDatabase;

/// Outcome of merging one object-data file.
#[derive(Debug, Clone)]
pub struct MergeOutcome {
    pub source: String,
    pub kind: ObjectKind,
    pub base_entries: usize,
    pub custom_entries: usize,
    pub report: MergeReport,
}

/// Pre-parsed layers for [`GameDatabase::from_parts`].
///
/// This is the bridge used by the composed install database
/// (`wc3-re::objectdata::ObjectDb`): that layer already parsed every SLK, text
/// file and metadata sheet, so the typed database is built from those same
/// values instead of re-reading or re-parsing bytes. Nothing here carries a
/// second parser — every element is the canonical `wc3-formats::gamedata` type.
#[derive(Debug, Default)]
pub struct DatabaseParts {
    /// Description of where the bytes came from (`mpq:war3.mpq`, `dir:…`).
    pub root: String,
    /// Parsed SLK tables, in any order.
    pub tables: Vec<(TableKind, wc3_formats::gamedata::Sheet)>,
    /// Parsed metadata sheets (`UnitMetaData`, `AbilityMetaData`, …).
    pub meta: Vec<(MetaKind, MetaTable)>,
    /// Parsed profile text layers: `(name, ordered sections)`.
    pub profiles: Vec<(String, Vec<(String, Vec<(String, String)>)>)>,
    /// Parsed misc/UI layers: `(name, ordered sections)`.
    pub misc: Vec<(String, Vec<(String, Vec<(String, String)>)>)>,
    /// `UI\UnitEditorData.txt` sections (editor enum dictionaries).
    pub enums: Option<Vec<(String, Vec<(String, String)>)>>,
    /// `UI\WorldEditStrings.txt` sections (`WESTRING_*`).
    pub westrings: Option<Vec<(String, Vec<(String, String)>)>>,
    /// Object-data text overrides already applied by the caller.
    pub profile_overlay: wc3_formats::gamedata::ProfileOverlay,
    /// Map string table entries (`TRIGSTR_n → text`), already unescaped by the
    /// resource layer or parsed from `war3map.wts` bytes.
    pub wts_entries: Option<BTreeMap<u32, String>>,
    /// Archive listfile for path validation.
    pub catalog: Option<Listfile>,
    /// Optional files the source did not provide (reported only).
    pub missing: Vec<String>,
}

/// The typed WC3 game database (`GameDatabase` in `analysis/tech-lead/BASE.md`).
#[derive(Debug, Default)]
pub struct GameDatabase {
    root: String,
    tables: [Option<Table>; 10],
    meta: BTreeMap<MetaKind, MetaTable>,
    localizer: Localizer,
    enums: Option<EditorEnums>,
    misc: MiscTables,
    catalog: Option<Listfile>,
    object_data: ObjectDataStore,
    merges: Vec<MergeOutcome>,
    missing: Vec<String>,
    buffs: BuffStore,
}

impl GameDatabase {
    /// Load every table/profile file the source provides.
    pub fn load(source: &dyn Source) -> Result<Self> {
        let mut data = Self {
            root: source.describe(),
            ..Self::default()
        };
        for kind in TableKind::ALL {
            match source.read(kind.path()) {
                Ok(bytes) => {
                    let table = Table::parse(kind, &bytes)?;
                    data.tables[kind.slot()] = Some(table);
                }
                Err(DataError::Missing { .. }) => data.missing.push(kind.path().to_string()),
                Err(error) => return Err(error),
            }
        }
        if data.tables.iter().all(Option::is_none) {
            return Err(DataError::Missing {
                path: "Units\\UnitData.slk".to_string(),
                root: std::path::PathBuf::from(&data.root),
            });
        }
        data.load_profiles(source)?;
        data.load_meta(source)?;
        data.enums = EditorEnums::load(source)?;
        data.misc = MiscTables::load(source)?;
        if source.exists("UI\\WorldEditStrings.txt") {
            let bytes = source.read("UI\\WorldEditStrings.txt")?;
            data.localizer.add_westrings(&bytes)?;
        }
        if source.exists("Units\\AbilityBuffData.slk") {
            let bytes = source.read("Units\\AbilityBuffData.slk")?;
            let table = Table::parse(TableKind::UnitData, &bytes)?;
            data.buffs.set_base(table);
        }
        Ok(data)
    }

    /// Build from already-parsed layers (see [`DatabaseParts`]).
    ///
    /// Fails only when a supplied sheet does not carry its key column, which
    /// means the caller paired the wrong sheet with a [`TableKind`].
    pub fn from_parts(parts: DatabaseParts) -> Result<Self> {
        let mut data = Self {
            root: parts.root,
            ..Self::default()
        };
        for (kind, sheet) in parts.tables {
            data.tables[kind.slot()] = Some(Table::from_sheet(kind, sheet)?);
        }
        for (kind, table) in parts.meta {
            data.meta.insert(kind, table);
        }
        for (name, sections) in parts.profiles {
            data.localizer.add_sections(name, sections);
        }
        for (name, sections) in parts.misc {
            data.misc.add_sections(name, sections);
        }
        if let Some(sections) = parts.enums {
            data.enums = Some(EditorEnums::from_sections(&sections));
        }
        if let Some(sections) = parts.westrings {
            data.localizer.add_westring_sections(&sections);
        }
        data.localizer.set_overlay(parts.profile_overlay);
        if let Some(entries) = parts.wts_entries {
            data.localizer.set_wts_entries(entries);
        }
        data.catalog = parts.catalog;
        data.missing = parts.missing;
        Ok(data)
    }

    /// Parse and merge an object-data file whose kind comes from its extension
    /// (`war3map.w3u`, `.w3a`, `.w3t`, `.w3q`, `.w3b`, `.w3d`, `.w3h`).
    pub fn apply_object_data_named(
        &mut self,
        path: impl Into<String>,
        bytes: &[u8],
    ) -> Result<ObjectKind> {
        let path = path.into();
        let extension = path
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .unwrap_or_default();
        let kind = ObjectKind::from_extension(extension)
            .ok_or_else(|| DataError::UnknownObjectKind { path: path.clone() })?;
        self.apply_object_data(path, kind, bytes)?;
        Ok(kind)
    }

    /// Attach an archive listfile used by path validation.
    pub fn with_catalog(mut self, catalog: Listfile) -> Self {
        self.catalog = Some(catalog);
        self
    }

    pub fn set_catalog(&mut self, catalog: Listfile) {
        self.catalog = Some(catalog);
    }

    fn load_profiles(&mut self, source: &dyn Source) -> Result<()> {
        for suffix in ["Strings", "Func"] {
            for base in PROFILE_BASES {
                let path = format!("Units\\{base}{suffix}.txt");
                if source.exists(&path) {
                    let bytes = source.read(&path)?;
                    self.localizer.add_profile(path.clone(), &bytes)?;
                } else {
                    self.localizer.note_missing(path);
                }
            }
        }
        Ok(())
    }

    fn load_meta(&mut self, source: &dyn Source) -> Result<()> {
        for kind in META_KINDS {
            let path = format!("Units\\{}", kind.sheet_name());
            if !source.exists(&path) {
                continue;
            }
            let bytes = source.read(&path)?;
            let table = MetaTable::parse(kind, &bytes)
                .map_err(|error| DataError::format(path, error))?;
            self.meta.insert(kind, table);
        }
        Ok(())
    }

    // ---- accessors ------------------------------------------------------

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn table(&self, kind: TableKind) -> Option<&Table> {
        self.tables[kind.slot()].as_ref()
    }

    pub fn table_mut(&mut self, kind: TableKind) -> Option<&mut Table> {
        self.tables[kind.slot()].as_mut()
    }

    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.tables.iter().flatten()
    }

    /// Metadata table for `kind`, falling back to `UnitMetaData` for items
    /// (retail ships no `ItemMetaData.slk`; item fields live in the unit sheet).
    pub fn meta(&self, kind: MetaKind) -> Option<&MetaTable> {
        self.meta.get(&kind).or_else(|| {
            kind.item_fallback()
                .and_then(|fallback| self.meta.get(&fallback))
        })
    }

    pub fn meta_table(&self, kind: MetaKind) -> Option<&MetaTable> {
        self.meta.get(&kind)
    }

    pub fn localizer(&self) -> &Localizer {
        &self.localizer
    }

    pub fn localizer_mut(&mut self) -> &mut Localizer {
        &mut self.localizer
    }

    pub fn enums(&self) -> Option<&EditorEnums> {
        self.enums.as_ref()
    }

    pub fn misc(&self) -> &MiscTables {
        &self.misc
    }

    pub fn catalog(&self) -> Option<&Listfile> {
        self.catalog.as_ref()
    }

    pub fn object_data(&self) -> &ObjectDataStore {
        &self.object_data
    }

    pub fn merges(&self) -> &[MergeOutcome] {
        &self.merges
    }

    pub fn buffs(&self) -> &BuffStore {
        &self.buffs
    }

    /// Files the source did not provide (reported, never fatal).
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Counted data skips since process start (`missing_slk_column`,
    /// `slk_position_out_of_range`, `profile_component_index`, …).
    ///
    /// The values are process-wide because the format layer counts where the
    /// bytes are read; the load report pairs this with [`Self::counts`].
    pub fn data_skips(&self) -> Vec<(&'static str, u64)> {
        wc3_formats::diag::snapshot()
    }

    /// One-line aggregate of [`Self::data_skips`]; empty when nothing skipped.
    pub fn data_skip_summary(&self) -> String {
        wc3_formats::diag::summary()
    }

    /// Number of loaded rows per table.
    pub fn counts(&self) -> Vec<(TableKind, usize)> {
        self.tables
            .iter()
            .flatten()
            .map(|table| (table.kind(), table.len()))
            .collect()
    }

    // ---- lookups --------------------------------------------------------

    pub fn unit(&self, id: &str) -> Option<UnitView<'_>> {
        let table = self.table(TableKind::UnitData)?;
        let parsed = wc3_formats::gamedata::FourCc::parse(id)?;
        table.contains(id).then_some(UnitView { gd: self, id: parsed })
    }

    pub fn units(&self) -> impl Iterator<Item = UnitView<'_>> {
        let ids: Vec<wc3_formats::gamedata::FourCc> = self
            .table(TableKind::UnitData)
            .map(|table| {
                table
                    .ids()
                    .iter()
                    .filter_map(|id| wc3_formats::gamedata::FourCc::parse(id))
                    .collect()
            })
            .unwrap_or_default();
        ids.into_iter().map(move |id| UnitView { gd: self, id })
    }

    pub fn ability(&self, id: &str) -> Option<AbilityView<'_>> {
        let table = self.table(TableKind::AbilityData)?;
        let parsed = wc3_formats::gamedata::FourCc::parse(id)?;
        table
            .contains(id)
            .then_some(AbilityView { gd: self, id: parsed })
    }

    pub fn abilities(&self) -> impl Iterator<Item = AbilityView<'_>> {
        let ids = self.ids_of(TableKind::AbilityData);
        ids.into_iter()
            .map(move |id| AbilityView { gd: self, id })
    }

    pub fn item(&self, id: &str) -> Option<ItemView<'_>> {
        let table = self.table(TableKind::ItemData)?;
        let parsed = wc3_formats::gamedata::FourCc::parse(id)?;
        table.contains(id).then_some(ItemView { gd: self, id: parsed })
    }

    pub fn items(&self) -> impl Iterator<Item = ItemView<'_>> {
        let ids = self.ids_of(TableKind::ItemData);
        ids.into_iter().map(move |id| ItemView { gd: self, id })
    }

    pub fn upgrade(&self, id: &str) -> Option<UpgradeView<'_>> {
        let table = self.table(TableKind::UpgradeData)?;
        let parsed = wc3_formats::gamedata::FourCc::parse(id)?;
        table
            .contains(id)
            .then_some(UpgradeView { gd: self, id: parsed })
    }

    pub fn upgrades(&self) -> impl Iterator<Item = UpgradeView<'_>> {
        let ids = self.ids_of(TableKind::UpgradeData);
        ids.into_iter().map(move |id| UpgradeView { gd: self, id })
    }

    pub fn destructable(&self, id: &str) -> Option<DestructableView<'_>> {
        let table = self.table(TableKind::DestructableData)?;
        let parsed = wc3_formats::gamedata::FourCc::parse(id)?;
        table
            .contains(id)
            .then_some(DestructableView { gd: self, id: parsed })
    }

    pub fn destructables(&self) -> impl Iterator<Item = DestructableView<'_>> {
        let ids = self.ids_of(TableKind::DestructableData);
        ids.into_iter()
            .map(move |id| DestructableView { gd: self, id })
    }

    pub fn doodad(&self, id: &str) -> Option<DoodadView<'_>> {
        let table = self.table(TableKind::Doodads)?;
        let parsed = wc3_formats::gamedata::FourCc::parse(id)?;
        table.contains(id).then_some(DoodadView { gd: self, id: parsed })
    }

    pub fn doodads(&self) -> impl Iterator<Item = DoodadView<'_>> {
        let ids = self.ids_of(TableKind::Doodads);
        ids.into_iter().map(move |id| DoodadView { gd: self, id })
    }

    fn ids_of(&self, kind: TableKind) -> Vec<wc3_formats::gamedata::FourCc> {
        self.table(kind)
            .map(|table| {
                table
                    .ids()
                    .iter()
                    .filter_map(|id| wc3_formats::gamedata::FourCc::parse(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `(comment, dataType)` of an upgrade effect code (`rarm` …), from
    /// `UpgradeEffectMetaData.slk` when the root ships it.
    pub fn upgrade_effect(&self, code: Option<&str>) -> Option<(Option<String>, Option<String>)> {
        let code = code?;
        let meta = self.meta_table(MetaKind::UpgradeEffect)?;
        // `UpgradeEffectMetaData.slk` keys effects by the `effectID` column (two
        // rows per effect: "Base" and "Mod"), not by the `ID` column.
        let field = meta
            .fields()
            .find(|field| field.raw_column("effectID").map(str::trim) == Some(code))?;
        Some((
            field.raw_column("comment").map(str::to_string),
            field.raw_column("dataType").map(str::to_string),
        ))
    }

    // ---- content model --------------------------------------------------

    /// Tech requirements of any object row.
    ///
    /// Retail encodes the list two ways and both are read here, in order:
    /// units and items write `Requires=a,b` (measured: 56 unit sections in the
    /// RoC profile text), while upgrades write `Requirescount=N` with
    /// `Requires1..N` and an empty `Requires=`
    /// (`[Rhme] Requirescount=3`, `Requires=`, `Requires1=hkee`,
    /// `Requires2=hcas`). Entries that name no object (`_`, `-`) are dropped.
    ///
    /// The numbered slots are read until the first gap, so a map override that
    /// adds one more slot is picked up without trusting `Requirescount`.
    pub fn requirements(&self, id: &str) -> Vec<String> {
        let localizer = self.localizer();
        let mut out = localizer.list(id, "Requires");
        for index in 1..=32 {
            let key = format!("Requires{index}");
            let mut slot = localizer.list(id, &key);
            if slot.is_empty() {
                break;
            }
            out.append(&mut slot);
        }
        out
    }

    // ---- object data ----------------------------------------------------

    /// Parse and merge an object-data file into the loaded tables.
    pub fn apply_object_data(
        &mut self,
        source: impl Into<String>,
        kind: ObjectKind,
        bytes: &[u8],
    ) -> Result<&MergeOutcome> {
        let source = source.into();
        let file = ObjectDataFile::parse(kind, bytes)
            .map_err(|error| DataError::format(source.clone(), error))?;
        self.apply_parsed(source, file)
    }

    /// Merge an already parsed object-data file.
    pub fn apply_parsed(&mut self, source: String, file: ObjectDataFile) -> Result<&MergeOutcome> {
        let kind = file.kind;
        let meta = self.meta(kind.meta_kind()).cloned();
        // Sheets named by the metadata of the fields this file touches: unit
        // object data can edit `ItemData` (`isst` = item stock start).
        let mut named_sheets: Vec<String> = Vec::new();
        if let Some(meta) = meta.as_ref() {
            for entry in file.entries() {
                for change in &entry.changes {
                    if let Some(field) = meta.get(&change.id) {
                        if !field.is_profile() && !field.slk().is_empty() {
                            named_sheets.push(field.slk().to_ascii_lowercase());
                        }
                    }
                }
            }
        }
        let mut merge_targets: Vec<MergeTarget<'_>> = Vec::new();
        for slot in self.tables.iter_mut() {
            let Some(table) = slot.as_mut() else {
                continue;
            };
            let table_kind = table.kind();
            let primary = table_kind.object_kind() == kind;
            let named = named_sheets
                .iter()
                .any(|name| name.eq_ignore_ascii_case(table_kind.name()));
            if !primary && !named {
                continue;
            }
            // Metadata may name a sheet differently (`DoodadData` vs `Doodads`):
            // register the alias so `merge_overlay` can resolve it.
            let name = table_kind.aliases().first().copied().unwrap_or(table_kind.name());
            merge_targets.push(MergeTarget {
                name,
                sheet: table.sheet_mut(),
                create_rows: primary,
            });
        }
        let mut overlay = self.localizer.overlay().clone();
        let report = wc3_formats::gamedata::merge_overlay(
            kind,
            &file,
            meta.as_ref(),
            &mut merge_targets,
            &mut overlay,
        );
        self.localizer.set_overlay(overlay);
        // Buffs own no sheet: a `war3map.w3b` entry is a `BuffStore` overlay.
        if kind == ObjectKind::Buff {
            self.buffs.apply(&file, meta.as_ref());
        }
        self.object_data.add_parsed(source.clone(), file.clone());
        self.merges.push(MergeOutcome {
            source,
            kind,
            base_entries: file.base.len(),
            custom_entries: file.custom.len(),
            report,
        });
        // The push above is unconditional, so `last()` is `Some`; the match
        // keeps the function total without an `expect`.
        self.merges
            .last()
            .ok_or_else(|| DataError::Missing {
                path: "<merge outcome>".to_string(),
                root: std::path::PathBuf::from(&self.root),
            })
    }

    /// Attach a `.wts` string table (`war3map.wts`).
    pub fn set_wts(&mut self, bytes: &[u8]) -> Result<()> {
        self.localizer.set_wts(bytes)
    }

    /// Validate the whole data set.
    pub fn validate(&self) -> validate::Report {
        validate::validate(self)
    }
}

/// Values of a change as text (helper for tools).
pub fn change_text(value: &ChangeValue) -> String {
    value.as_text()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn minimal() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\UnitData.slk",
            b"ID;PWXL;N;E\nB;Y2;X3;D0\nC;Y1;X1;K\"unitID\"\nC;X2;K\"race\"\nC;X3;K\"type\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"human\"\nC;X3;K\"Mechanical\"\n"
                .to_vec(),
        );
        source
    }

    #[test]
    fn loads_partial_roots_and_reports_missing_files() {
        let data = GameDatabase::load(&minimal()).expect("load");
        assert_eq!(data.counts().len(), 1);
        assert!(data.unit("hfoo").is_some());
        assert!(data.unit("nope").is_none());
        assert!(data.missing().iter().any(|path| path.ends_with("UnitBalance.slk")));
        // 23 bases x 2 suffixes, minus the files we do not provide.
        assert!(data.localizer().missing_files().len() >= 40);
    }

    #[test]
    fn rejects_a_source_without_any_table() {
        let source = MemorySource::new();
        assert!(matches!(GameDatabase::load(&source), Err(DataError::Missing { .. })));
    }
}

/// P0 gap-close gates (2026-11-13): the QA parity binary measures three
/// conditions against this crate's public API. These tests pin the data counts
/// from the canonical gate (`cargo test -p wc3-data --lib`):
///
/// - `items.schema`: both item trees expose a class for every item row
///   (RoC writes `itemClass`, TFT writes `class`);
/// - `hero.coverage`: all twelve ladder heroes resolve to a full `HeroDef`
///   (the QA roster's own row for `Emoo` is a sim-table concern);
/// - `cast.coverage`: every roster ability id resolves to `AbilityData` numbers
///   (the sim's 11 hand-written `Spell` variants are a sim concern).
#[cfg(test)]
mod gap_close_tests {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use crate::source::DirSource;
    use crate::GameDatabase;

    fn analysis_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
    }

    fn load(root: &str) -> GameDatabase {
        GameDatabase::load(&DirSource::new(analysis_dir().join(root)))
            .unwrap_or_else(|error| panic!("load {root}: {error}"))
    }

    fn roc() -> &'static GameDatabase {
        static ROC: OnceLock<GameDatabase> = OnceLock::new();
        ROC.get_or_init(|| load("war3_data"))
    }

    fn tft() -> &'static GameDatabase {
        static TFT: OnceLock<GameDatabase> = OnceLock::new();
        TFT.get_or_init(|| load("war3x_data"))
    }

    /// `items.schema`: class column exposed on every item of both trees.
    #[test]
    fn class_column_is_exposed_for_both_item_trees() {
        for (label, data, expected) in [("roc", roc(), 132), ("tft", tft(), 273)] {
            let items: Vec<_> = data.items().collect();
            assert_eq!(items.len(), expected, "{label} item rows");
            let with_class = items.iter().filter(|item| item.class().is_some()).count();
            assert_eq!(with_class, expected, "{label} class exposure");
        }
    }

    /// `hero.coverage`: the twelve ladder heroes resolve from `UnitBalance`.
    #[test]
    fn melee_hero_coverage_is_twelve_of_twelve() {
        for (label, data) in [("roc", roc()), ("tft", tft())] {
            let (present, total) = data.melee_hero_coverage();
            assert_eq!((present, total), (12, 12), "{label} ladder heroes");
            let emoo = data.hero("Emoo").expect("Emoo hero def");
            assert_eq!(emoo.skills.len(), 4, "{label} Emoo skills");
            assert!(emoo.attributes.intelligence > 0, "{label} Emoo INT");
            assert!(emoo.hp_at(1) > 0, "{label} Emoo hp");
            assert!(emoo.mana_at(1) > 0, "{label} Emoo mana");
            for id in crate::hero::LADDER_HERO_IDS {
                assert!(data.hero(id).is_some(), "{label} {id} hero def");
            }
        }
    }

    /// `cast.coverage`: roster ability ids have `AbilityData` numbers; counts are
    /// measured, not asserted as a constant.
    #[test]
    fn castable_ability_coverage_covers_the_roster_population() {
        for (label, data, expected_total) in [("roc", roc(), 252), ("tft", tft(), 442)] {
            let (covered, total) = data.castable_ability_coverage();
            assert_eq!(total, expected_total, "{label} referenced ability ids");
            assert!(covered >= total - 1, "{label} covered {covered}/{total}");
            // Ids the QA gate reported as "not simulated" must at least carry
            // sheet numbers — the data side of castability.
            for id in ["Acrs", "Aenc", "Apts", "AOre", "AUin", "AHab", "AOae", "Adef"] {
                let numbers = data
                    .ability_numbers(id)
                    .unwrap_or_else(|| panic!("{label} {id} numbers"));
                assert!(!numbers.levels.is_empty(), "{label} {id} levels");
            }
        }
    }
}
