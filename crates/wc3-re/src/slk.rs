//! SLK spreadsheet access with row-key lookup.
//!
//! Warcraft III SLK files are `ID;PWXL` spreadsheets: `C;` records set the
//! current X/Y cursor and store `K"value"` (quoted, `""` = literal quote) or
//! `K<number>` cells. Row 1 holds column names; every following row is an
//! object record whose first column is the four-character ID (`unitID`,
//! `alias`, `itemID`, `upgradeid`, …). `E` records are empty-cell runs and are
//! ignored. Parsing is delegated to [`wc3_formats::slk::SlkTable`]; this module
//! preserves the full grid plus the resource-layer row-key index so unknown
//! columns survive a game-version change.

use std::collections::BTreeMap;

use crate::error::{ReError, Result};

#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_keys: Vec<String>,
    index: BTreeMap<String, usize>,
}

impl Sheet {
    pub fn parse(name: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let name: String = name.into();
        // Parsing is owned by the format layer (`wc3-formats::slk`). This type
        // adds the resource-layer view: the row-key index and per-record maps
        // used by the object database and the install scanner.
        let table = wc3_formats::slk::SlkTable::parse(bytes)
            .map_err(|err| ReError::Parse(format!("SLK {name}: {err}")))?;
        let mut index = BTreeMap::new();
        let mut row_keys = Vec::with_capacity(table.rows.len());
        for (row, values) in table.rows.iter().enumerate() {
            let key = values.first().cloned().unwrap_or_default();
            if !key.is_empty() {
                index.entry(key.clone()).or_insert(row);
            }
            row_keys.push(key);
        }
        Ok(Self {
            name,
            columns: table.columns,
            rows: table.rows,
            row_keys,
            index,
        })
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(name))
    }

    /// Row index for a four-character object id (first column).
    pub fn row_index(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    pub fn cell(&self, row: usize, column: &str) -> Option<&str> {
        let index = self.column_index(column)?;
        self.rows.get(row)?.get(index).map(String::as_str)
    }

    pub fn cell_at(&self, row: usize, column: usize) -> Option<&str> {
        self.rows.get(row)?.get(column).map(String::as_str)
    }

    pub fn record(&self, row: usize) -> BTreeMap<&str, &str> {
        let mut map = BTreeMap::new();
        if let Some(values) = self.rows.get(row) {
            for (index, column) in self.columns.iter().enumerate() {
                if let Some(value) = values.get(index) {
                    map.insert(column.as_str(), value.as_str());
                }
            }
        }
        map
    }

    /// Column names, in spreadsheet order.
    pub fn column_names(&self) -> &[String] {
        &self.columns
    }

    /// `(row_key, row)` iterator.
    pub fn records(&self) -> impl Iterator<Item = (&str, &[String])> + '_ {
        self.row_keys
            .iter()
            .map(String::as_str)
            .zip(self.rows.iter().map(Vec::as_slice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_real_unit_data() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Units/UnitData.slk");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skip: UnitData.slk missing");
            return;
        };
        let sheet = Sheet::parse("UnitData", &bytes).expect("parse");
        assert!(sheet.columns.iter().any(|c| c == "unitID"));
        assert!(sheet.len() > 100, "rows {}", sheet.len());
        let row = sheet.row_index("hfoo").expect("footman row");
        assert_eq!(sheet.cell(row, "unitID"), Some("hfoo"));
    }
}
