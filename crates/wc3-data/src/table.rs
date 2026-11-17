//! Typed table registry: one entry per shipped SLK sheet.

use wc3_formats::gamedata::{Cell, ObjectKind, Sheet};

use crate::error::{DataError, Result};

/// Shipped data sheets that carry object definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TableKind {
    UnitData,
    UnitBalance,
    UnitWeapons,
    UnitAbilities,
    UnitUi,
    AbilityData,
    ItemData,
    UpgradeData,
    DestructableData,
    Doodads,
}

impl TableKind {
    pub const ALL: [TableKind; 10] = [
        TableKind::UnitData,
        TableKind::UnitBalance,
        TableKind::UnitWeapons,
        TableKind::UnitAbilities,
        TableKind::UnitUi,
        TableKind::AbilityData,
        TableKind::ItemData,
        TableKind::UpgradeData,
        TableKind::DestructableData,
        TableKind::Doodads,
    ];

    /// Name used by `*MetaData.slk` in its `slk` column (the object-data merge
    /// key). `observed` in `analysis/war3_data/Units/*MetaData.slk` and
    /// `analysis/war3x_data/Units/*MetaData.slk`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::UnitData => "UnitData",
            Self::UnitBalance => "UnitBalance",
            Self::UnitWeapons => "UnitWeapons",
            Self::UnitAbilities => "UnitAbilities",
            Self::UnitUi => "UnitUI",
            Self::AbilityData => "AbilityData",
            Self::ItemData => "ItemData",
            Self::UpgradeData => "UpgradeData",
            Self::DestructableData => "DestructableData",
            Self::Doodads => "Doodads",
        }
    }

    /// Additional names metadata sheets use for this table
    /// (`DoodadMetaData.slk` says `DoodadData`).
    pub fn aliases(&self) -> &'static [&'static str] {
        match self {
            Self::Doodads => &["DoodadData"],
            _ => &[],
        }
    }

    /// In-archive path.
    pub fn path(&self) -> &'static str {
        match self {
            Self::UnitData => "Units\\UnitData.slk",
            Self::UnitBalance => "Units\\UnitBalance.slk",
            Self::UnitWeapons => "Units\\UnitWeapons.slk",
            Self::UnitAbilities => "Units\\UnitAbilities.slk",
            Self::UnitUi => "Units\\unitUI.slk",
            Self::AbilityData => "Units\\AbilityData.slk",
            Self::ItemData => "Units\\ItemData.slk",
            Self::UpgradeData => "Units\\UpgradeData.slk",
            Self::DestructableData => "Units\\DestructableData.slk",
            Self::Doodads => "Doodads\\Doodads.slk",
        }
    }

    /// Primary key column.
    ///
    /// The shipped headers disagree on spelling (`unitID`, `upgradeid`,
    /// `DestructableID`, `doodID`, `alias`), so the exact header string is
    /// recorded here instead of being guessed. Column lookup is
    /// case-insensitive, which covers `unitWeapID` vs `unitweapid`.
    pub fn id_column(&self) -> &'static str {
        match self {
            Self::UnitData => "unitID",
            Self::UnitBalance => "unitBalanceID",
            Self::UnitWeapons => "unitWeapID",
            Self::UnitAbilities => "unitAbilID",
            Self::UnitUi => "unitUIID",
            Self::AbilityData => "alias",
            Self::ItemData => "itemID",
            Self::UpgradeData => "upgradeid",
            Self::DestructableData => "DestructableID",
            Self::Doodads => "doodID",
        }
    }

    /// Object-data kind whose modifications may target this sheet.
    pub fn object_kind(&self) -> ObjectKind {
        match self {
            Self::UnitData
            | Self::UnitBalance
            | Self::UnitWeapons
            | Self::UnitAbilities
            | Self::UnitUi => ObjectKind::Unit,
            Self::AbilityData => ObjectKind::Ability,
            Self::ItemData => ObjectKind::Item,
            Self::UpgradeData => ObjectKind::Upgrade,
            Self::DestructableData => ObjectKind::Destructable,
            Self::Doodads => ObjectKind::Doodad,
        }
    }

    /// Position in [`TableKind::ALL`], for dense storage.
    pub const fn slot(&self) -> usize {
        match self {
            Self::UnitData => 0,
            Self::UnitBalance => 1,
            Self::UnitWeapons => 2,
            Self::UnitAbilities => 3,
            Self::UnitUi => 4,
            Self::AbilityData => 5,
            Self::ItemData => 6,
            Self::UpgradeData => 7,
            Self::DestructableData => 8,
            Self::Doodads => 9,
        }
    }

    pub fn from_slot(slot: usize) -> Option<Self> {
        Self::ALL.get(slot).copied()
    }

    /// Reverse of [`TableKind::name`] including metadata aliases
    /// (`DoodadMetaData.slk` calls the doodad sheet `DoodadData`) and the
    /// `unitUI.slk` spelling difference.
    pub fn from_sheet_name(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL.into_iter().find(|kind| {
            kind.name().eq_ignore_ascii_case(name)
                || kind
                    .aliases()
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
                || (kind.path().rsplit('\\').next().unwrap_or("").eq_ignore_ascii_case(name))
                || kind
                    .path()
                    .rsplit('\\')
                    .next()
                    .and_then(|file| file.rsplit_once('.').map(|(stem, _)| stem))
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(name))
        })
    }
}

/// A parsed table plus schema metadata.
#[derive(Debug, Clone)]
pub struct Table {
    kind: TableKind,
    sheet: Sheet,
    id_column: usize,
}

impl Table {
    pub fn parse(kind: TableKind, bytes: &[u8]) -> Result<Self> {
        let sheet = Sheet::parse(kind.name(), bytes)
            .map_err(|source| DataError::format(kind.path(), source))?;
        let id_column = sheet.column_index(kind.id_column()).ok_or_else(|| {
            DataError::MissingColumn {
                table: kind.name().to_string(),
                column: kind.id_column().to_string(),
            }
        })?;
        Ok(Self {
            kind,
            sheet,
            id_column,
        })
    }

    /// Build from an already-parsed sheet (no byte re-read).
    ///
    /// Used by the composed install database (`wc3-re::objectdata`) so the typed
    /// database and the engine read the same parsed cells.
    pub fn from_sheet(kind: TableKind, sheet: Sheet) -> Result<Self> {
        let id_column = sheet.column_index(kind.id_column()).ok_or_else(|| {
            DataError::MissingColumn {
                table: kind.name().to_string(),
                column: kind.id_column().to_string(),
            }
        })?;
        Ok(Self {
            kind,
            sheet,
            id_column,
        })
    }

    pub fn kind(&self) -> TableKind {
        self.kind
    }

    pub fn sheet(&self) -> &Sheet {
        &self.sheet
    }

    pub fn sheet_mut(&mut self) -> &mut Sheet {
        &mut self.sheet
    }

    pub fn len(&self) -> usize {
        self.sheet.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sheet.is_empty()
    }

    pub fn ids(&self) -> &[String] {
        self.sheet.row_ids()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.sheet.contains(id)
    }

    pub fn id_column(&self) -> usize {
        self.id_column
    }

    pub fn has_column(&self, column: &str) -> bool {
        self.sheet.column_index(column).is_some()
    }

    pub fn column_index(&self, column: &str) -> Option<usize> {
        self.sheet.column_index(column)
    }

    pub fn columns(&self) -> &[String] {
        self.sheet.columns()
    }

    /// Cell as the game sees it: object-data overlays win over the shipped SLK.
    pub fn cell(&self, id: &str, column: &str) -> Option<Cell<'_>> {
        self.cell_overlaid(id, column)
    }

    /// Cell including object-data overlay values.
    pub fn cell_overlaid(&self, id: &str, column: &str) -> Option<Cell<'_>> {
        self.note_missing_column(column);
        self.sheet
            .cell_with_overlay(id, column)
            .map(|(cell, _origin)| cell)
    }

    /// Cell exactly as shipped in the SLK (ignores overlays).
    pub fn cell_base(&self, id: &str, column: &str) -> Option<Cell<'_>> {
        self.note_missing_column(column);
        self.sheet.cell(id, column)
    }

    /// Text value with `-`/`_` treated as absent (overlay wins).
    pub fn text(&self, id: &str, column: &str) -> Option<&str> {
        self.cell_overlaid(id, column).and_then(|cell| cell.text())
    }

    /// Report a column the sheet does not carry.
    ///
    /// Typed readers return the caller's default for an absent column; the
    /// count here is what tells a load report whether the default came from a
    /// schema mismatch (wrong or older sheet) instead of from real data. The
    /// diagnostic channel bounds the emitted lines, so a per-row miss stays
    /// cheap.
    fn note_missing_column(&self, column: &str) {
        let in_base = self.sheet.column_index(column).is_some();
        let in_shadow = self
            .sheet
            .shadow_columns()
            .iter()
            .any(|name| name.eq_ignore_ascii_case(column));
        if !in_base && !in_shadow {
            wc3_formats::diag::note("missing_slk_column", || {
                format!("table={} column={column}", self.kind.name())
            });
        }
    }

    /// Whether `column` exists in the base SLK or was created by an overlay.
    pub fn column_present(&self, column: &str) -> bool {
        self.sheet.column_index(column).is_some()
            || self
                .sheet
                .shadow_columns()
                .iter()
                .any(|name| name.eq_ignore_ascii_case(column))
    }

    /// Text exactly as shipped (ignores overlays).
    pub fn text_base(&self, id: &str, column: &str) -> Option<&str> {
        self.cell_base(id, column).and_then(|cell| cell.text())
    }

    pub fn text_overlaid(&self, id: &str, column: &str) -> Option<&str> {
        self.cell_overlaid(id, column).and_then(|cell| cell.text())
    }

    fn bad_cell(&self, id: &str, column: &str, cell: Cell<'_>) -> DataError {
        DataError::BadCell {
            table: self.kind.name().to_string(),
            row: id.to_string(),
            column: column.to_string(),
            value: cell.raw().to_string(),
        }
    }

    /// Integer cell using the engine's `atof`-style reading.
    ///
    /// The shipped tables hold a few cells with trailing garbage
    /// (`0.5;ERC[-2]/400` in `UnitBalance.regenMana`); the game takes the leading
    /// number, so typed readers do the same. Use [`Table::i32_strict`] when
    /// validating the raw text.
    pub fn i32(&self, id: &str, column: &str) -> Result<Option<i32>> {
        let Some(cell) = self.cell_overlaid(id, column) else {
            return Ok(None);
        };
        if let Some(strict) = cell.i32(self.kind.name(), id, column).ok().flatten() {
            return Ok(Some(strict));
        }
        Ok(cell.i32_lossy())
    }

    /// Strict integer read: a cell with trailing text is an error.
    pub fn i32_strict(&self, id: &str, column: &str) -> Result<Option<i32>> {
        let Some(cell) = self.sheet.cell(id, column) else {
            return Ok(None);
        };
        cell.i32(self.kind.name(), id, column)
            .map_err(|_| self.bad_cell(id, column, cell))
    }

    /// Integer cell with overlay priority, read with the engine's `atof`
    /// semantics (leading number wins).
    ///
    /// The shipped sheets carry spreadsheet formula remnants in computed
    /// columns — `UnitBalance.realHP` is `650;ERC[-1]+…*25`, `realM` is
    /// `255;EIF(…)*15` — and the engine reads the leading number. Strict reads
    /// stay available through [`Table::i32_strict`] for validation.
    pub fn i32_overlaid(&self, id: &str, column: &str) -> Result<Option<i32>> {
        let Some(cell) = self.cell_overlaid(id, column) else {
            return Ok(None);
        };
        if let Some(strict) = cell.i32(self.kind.name(), id, column).ok().flatten() {
            return Ok(Some(strict));
        }
        Ok(cell.i32_lossy())
    }

    /// Float cell with overlay priority and engine `atof` semantics.
    pub fn f32_overlaid(&self, id: &str, column: &str) -> Result<Option<f32>> {
        let Some(cell) = self.cell_overlaid(id, column) else {
            return Ok(None);
        };
        if let Some(strict) = cell.f32(self.kind.name(), id, column).ok().flatten() {
            return Ok(Some(strict));
        }
        Ok(cell.f32_lossy())
    }

    pub fn u32(&self, id: &str, column: &str) -> Result<Option<u32>> {
        Ok(self.i32(id, column)?.and_then(|v| u32::try_from(v).ok()))
    }

    /// Float cell using the engine's `atof`-style reading.
    pub fn f32(&self, id: &str, column: &str) -> Result<Option<f32>> {
        let Some(cell) = self.cell_overlaid(id, column) else {
            return Ok(None);
        };
        if let Some(strict) = cell.f32(self.kind.name(), id, column).ok().flatten() {
            return Ok(Some(strict));
        }
        Ok(cell.f32_lossy())
    }

    /// Strict float read: a cell with trailing text is an error.
    pub fn f32_strict(&self, id: &str, column: &str) -> Result<Option<f32>> {
        let Some(cell) = self.sheet.cell(id, column) else {
            return Ok(None);
        };
        cell.f32(self.kind.name(), id, column)
            .map_err(|_| self.bad_cell(id, column, cell))
    }

    pub fn bool(&self, id: &str, column: &str) -> Result<Option<bool>> {
        let Some(cell) = self.cell_overlaid(id, column) else {
            return Ok(None);
        };
        cell.bool(self.kind.name(), id, column)
            .map_err(|_| self.bad_cell(id, column, cell))
    }

    pub fn list(&self, id: &str, column: &str) -> Vec<&str> {
        self.cell_overlaid(id, column)
            .map(|cell| cell.list())
            .unwrap_or_default()
    }

    /// Number of rows with a non-absent value in `column`.
    pub fn count_non_empty(&self, column: &str) -> usize {
        let Some(index) = self.sheet.column_index(column) else {
            return 0;
        };
        (0..self.sheet.len())
            .filter(|row| {
                self.sheet
                    .raw_cell(*row, index)
                    .map(|cell| !cell.is_absent())
                    .unwrap_or(false)
            })
            .count()
    }

    /// Distinct non-absent values of `column`.
    pub fn distinct(&self, column: &str) -> Vec<String> {
        let Some(index) = self.sheet.column_index(column) else {
            return Vec::new();
        };
        let mut seen = std::collections::BTreeSet::new();
        for row in 0..self.sheet.len() {
            if let Some(cell) = self.sheet.raw_cell(row, index) {
                if let Some(text) = cell.text() {
                    seen.insert(text.to_string());
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Row index of an object.
    pub fn row_of(&self, id: &str) -> Option<usize> {
        self.sheet.id_index(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &[u8] = b"ID;PWXL;N;E\nB;Y3;X4;D0\n\
C;Y1;X1;K\"unitID\"\nC;X2;K\"HP\"\nC;X3;K\"abilList\"\nC;X4;K\"isbldg\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K420\nC;X3;K\"Adef,Ahrl\"\nC;X4;K0\n\
C;Y3;X1;K\"htow\"\nC;X2;K1500\nC;X3;K-\nC;X4;K1\n";

    #[test]
    fn parses_and_types_cells() {
        let table = Table::parse(TableKind::UnitData, SHEET).expect("parse");
        assert_eq!(table.len(), 2);
        assert_eq!(table.ids(), ["hfoo", "htow"]);
        assert_eq!(table.text("hfoo", "HP"), Some("420"));
        assert_eq!(table.i32("hfoo", "hp").unwrap(), Some(420));
        assert_eq!(table.bool("htow", "isbldg").unwrap(), Some(true));
        assert_eq!(table.list("hfoo", "abilList"), ["Adef", "Ahrl"]);
        assert!(table.list("htow", "abilList").is_empty());
        assert_eq!(table.count_non_empty("abilList"), 1);
        assert_eq!(table.distinct("isbldg"), ["0", "1"]);
    }

    #[test]
    fn missing_column_is_reported_at_parse_time() {
        let broken = b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"nope\"\nC;X2;K\"HP\"\nC;Y2;X1;K\"hfoo\"\nC;X2;K1\n";
        match Table::parse(TableKind::UnitData, broken) {
            Err(DataError::MissingColumn { table, column }) => {
                assert_eq!(table, "UnitData");
                assert_eq!(column, "unitID");
            }
            other => panic!("expected missing column, got {other:?}"),
        }
    }

    #[test]
    fn non_numeric_cell_behaviour() {
        let table = Table::parse(TableKind::UnitData, SHEET).expect("parse");
        // `abilList` is text: the engine-style reader yields `None`, the strict
        // reader fails loudly.
        assert_eq!(table.i32("hfoo", "abilList").unwrap(), None);
        assert!(matches!(
            table.i32_strict("hfoo", "abilList"),
            Err(DataError::BadCell { .. })
        ));
        // A cell with a leading number and trailing text reads the number.
        assert_eq!(table.f32("hfoo", "HP").unwrap(), Some(420.0));
    }
}
