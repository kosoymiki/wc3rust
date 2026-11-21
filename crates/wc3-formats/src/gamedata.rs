//! Typed game-data layer: strict SLK sheets, object-data overlays, metadata and
//! `.wts` string tables.
//!
//! This module is the *format* half of the WC3 game database. It knows how bytes
//! become typed cells and how object data (`war3map.w3u`, `.w3a`, …) modifies
//! those cells. It deliberately carries **no** game semantics: names such as
//! "gold cost" or "attack range" live in `wc3-data`, which drives this layer
//! through `MetaTable` field descriptors.
//!
//! # Provenance of the models implemented here
//!
//! | Model | Source (donor tree, read-only) |
//! |---|---|
//! | SLK cell addressing (`C;X/Y;K`), header lowercasing, `-`/`_` empties, headerless-row removal | `donors/HiveWE/src/file_formats/slk.ixx:60-190` |
//! | Metadata → column resolution (field name, `data_pointer` letter, `repeat` level suffix) | `donors/HiveWE/src/utilities/modification_tables.ixx:16-80` |
//! | Object-data binary layout (version, base section, custom section, v3 set list) | `donors/WarsmashModEngine/core/src/com/etheller/warsmash/units/custom/War3ObjectDataChangeset.java:484-600` |
//! | Per-kind extended changes (ability/upgrade: level+dataptr, doodad: variation+dataptr) | same file `:230-245` (`extended()`) |
//! | Value type codes 0=int, 1=real, 2=unreal, 3=string, 4=bool(char obsolete) | `donors/War3Net/src/War3Net.Build.Core/Object/ObjectDataType.cs` |
//! | `TRIGSTR_` inline replacement | `War3ObjectDataChangeset.java:556-570`, `.wts` grammar in `units/custom/WTSFile.java` |
//!
//! Everything is validated against the extracted tables in `analysis/war3_data`
//! and `analysis/war3x_data` (see `wc3-data` tests).

use std::collections::HashMap;
use std::fmt;

use crate::FormatError;

/// Number of bytes in a WC3 object identifier ("FourCC").
pub const ID_LEN: usize = 4;

/// Hard ceiling on level/data-pointer components of one overlay string.
///
/// Real data is `level` (≤ 100 in shipped abilities) plus a data pointer
/// letter; 1024 components is already unreachable and keeps a forged index
/// from growing the vector without bound.
pub const MAX_PROFILE_COMPONENTS: usize = 1024;

/// Hard ceiling on SLK rows and columns.
///
/// The largest shipped sheet (`AbilityData.slk`, ~8 k rows) and the widest
/// (`UnitWeapons.slk`, ~200 columns) are far below this. A `C;Y…` record past
/// the ceiling is a corrupt position, not a real grid.
const MAX_SHEET_ROWS: usize = 65_536;
const MAX_SHEET_COLUMNS: usize = 1_024;
/// Total grid slots one sheet may materialise (WC3-SLK-001). The two per-axis
/// limits multiply out to 67 M slots (~1.6 GB of `Option<String>`); the shipped
/// sheets are far smaller, and the dense grid is only a convenience view over
/// the sparse stream.
const MAX_SHEET_CELLS: usize = 1_048_576;

/// Canonical FourCC (object identifier such as `hfoo`).
///
/// WC3 stores identifiers as four raw bytes. They are not always ASCII
/// (`phea` is, but map files may contain high bytes), so the raw form is kept
/// verbatim and only *display* assumes UTF-8 lossy.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FourCc(pub [u8; ID_LEN]);

impl FourCc {
    /// Build from four raw bytes.
    pub const fn new(bytes: [u8; ID_LEN]) -> Self {
        Self(bytes)
    }

    /// Parse exactly four bytes from `text`; longer or shorter text is rejected.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes: [u8; ID_LEN] = text.as_bytes().try_into().ok()?;
        Some(Self(bytes))
    }

    /// Parse, accepting the all-zero identifier used by "new object" sections.
    pub fn parse_or_zero(text: &str) -> Option<Self> {
        if text.is_empty() || text == "\0\0\0\0" {
            return Some(Self([0; ID_LEN]));
        }
        Self::parse(text)
    }

    pub fn bytes(&self) -> &[u8; ID_LEN] {
        &self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; ID_LEN]
    }

    /// Lossy text form (always four characters).
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

impl fmt::Debug for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FourCc({:?})", self.as_str())
    }
}

impl fmt::Display for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// Errors produced by this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GamedataError {
    /// Underlying container format problem (delegated to `crate::FormatError`).
    Format(FormatError),
    /// A required column is absent from a sheet.
    MissingColumn {
        sheet: String,
        column: String,
    },
    /// A row identifier is absent from a sheet.
    MissingRow {
        sheet: String,
        id: String,
    },
    /// A cell could not be converted to the requested numeric type.
    BadCell {
        sheet: String,
        row: String,
        column: String,
        value: String,
        expected: &'static str,
    },
    /// Sheet shape contradicts its own `B;` bounds record.
    BoundsMismatch {
        sheet: String,
        declared_rows: usize,
        declared_columns: usize,
        parsed_rows: usize,
        parsed_columns: usize,
    },
    /// Object-data file used a version this crate does not implement.
    UnsupportedVersion {
        file: &'static str,
        version: u32,
    },
    /// Object-data file declared an impossible count (corruption guard).
    CountLimit {
        what: &'static str,
        count: usize,
        limit: usize,
    },
    /// `.wts` file structure error.
    Wts(String),
}

impl fmt::Display for GamedataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(e) => write!(f, "format error: {e}"),
            Self::MissingColumn { sheet, column } => {
                write!(f, "sheet `{sheet}` has no column `{column}`")
            }
            Self::MissingRow { sheet, id } => write!(f, "sheet `{sheet}` has no row `{id}`"),
            Self::BadCell {
                sheet,
                row,
                column,
                value,
                expected,
            } => write!(
                f,
                "sheet `{sheet}` cell [{row}].{column} = {value:?} is not {expected}"
            ),
            Self::BoundsMismatch {
                sheet,
                declared_rows,
                declared_columns,
                parsed_rows,
                parsed_columns,
            } => write!(
                f,
                "sheet `{sheet}` bounds say {declared_rows}x{declared_columns} but parsed \
                 {parsed_rows}x{parsed_columns}"
            ),
            Self::UnsupportedVersion { file, version } => {
                write!(f, "{file}: unsupported object-data version {version}")
            }
            Self::CountLimit { what, count, limit } => {
                write!(f, "{what} count {count} exceeds limit {limit}")
            }
            Self::Wts(why) => write!(f, "wts: {why}"),
        }
    }
}

impl std::error::Error for GamedataError {}

impl From<FormatError> for GamedataError {
    fn from(value: FormatError) -> Self {
        Self::Format(value)
    }
}

/// Result alias for this module.
pub type Result<T> = std::result::Result<T, GamedataError>;

/// Declared sheet size from the `B;Y…;X…` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetBounds {
    pub rows: usize,
    pub columns: usize,
}

/// Why a numeric column holds non-numeric text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Artifact {
    /// Excel error text left in the sheet (`#VALUE!`, `0.5;ERC[-2]/400`).
    Spreadsheet,
    /// A FourCC marker where a number belongs (`MBUR`).
    Identifier,
}

/// A raw SLK cell.
///
/// WC3 uses `-` and `_` as "no value" markers (`donors/HiveWE/src/file_formats/slk.ixx:119`).
/// The raw text is preserved; [`Cell::is_absent`] applies that convention so
/// callers never lose the distinction between `""`, `-` and `_`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell<'a> {
    raw: &'a str,
}

impl<'a> Cell<'a> {
    pub fn new(raw: &'a str) -> Self {
        Self { raw }
    }

    /// Verbatim cell text.
    pub fn raw(&self) -> &'a str {
        self.raw
    }

    /// True for empty, `-` and `_` (the game's "no value" spellings).
    pub fn is_absent(&self) -> bool {
        let t = self.raw.trim();
        t.is_empty() || t == "-" || t == "_"
    }

    /// Text with surrounding whitespace removed, `None` when absent.
    pub fn text(&self) -> Option<&'a str> {
        if self.is_absent() {
            None
        } else {
            Some(self.raw.trim())
        }
    }

    /// Comma separated list (WC3 stores ability/upgrade/target lists this way).
    pub fn list(&self) -> Vec<&'a str> {
        match self.text() {
            None => Vec::new(),
            Some(text) => text
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .collect(),
        }
    }

    /// The leading numeric run of a cell (`0.5;ERC[-2]/400` → `0.5`).
    ///
    /// The engine converts cells with `atof`-style semantics, and the shipped
    /// tables contain a handful of cells with trailing garbage (Excel error
    /// artifacts in `UnitBalance.regenMana`), so typed readers accept a leading
    /// number while [`Cell::i32`] stays strict for validation.
    pub fn leading_number(&self) -> Option<f64> {
        let text = self.text()?;
        let bytes = text.as_bytes();
        let mut end = 0;
        let mut seen_digit = false;
        let mut seen_dot = false;
        let mut seen_exp = false;
        for (index, ch) in bytes.iter().enumerate() {
            match ch {
                b'+' | b'-' if index == 0 => {}
                b'0'..=b'9' => seen_digit = true,
                b'.' if !seen_dot && !seen_exp => seen_dot = true,
                b'e' | b'E' if seen_digit && !seen_exp => seen_exp = true,
                _ => break,
            }
            end = index + 1;
        }
        if !seen_digit {
            return None;
        }
        text[..end].parse::<f64>().ok()
    }

    /// True when the cell starts with a number followed by extra text
    /// (`0.5;ERC[-2]/400`). A cell with no number at all is *not* trailing
    /// garbage — callers treat that as a plain bad value.
    pub fn has_trailing_garbage(&self) -> bool {
        let Some(text) = self.text() else {
            return false;
        };
        match self.leading_number() {
            None => false,
            Some(_) => text.trim().parse::<f64>().is_err(),
        }
    }

    /// Classify cells that a spreadsheet or the editor left in a numeric column.
    ///
    /// The shipped tables contain Excel error artifacts (`#VALUE!`,
    /// `<number>;ER[-5]C`) and FourCC markers (`MBUR`) where numbers belong. The
    /// engine converts both to 0, so they are reported as data artifacts rather
    /// than as parser failures.
    pub fn artifact(&self) -> Option<Artifact> {
        let text = self.text()?.trim();
        const SPREADSHEET: [&str; 7] = [
            "#VALUE!", "#REF!", "#DIV/0!", "#N/A", "#NAME?", "#NUM!", "#NULL!",
        ];
        if SPREADSHEET.iter().any(|marker| text.starts_with(marker))
            || text.contains(";ER[")
            || text.contains(";ERC[")
        {
            return Some(Artifact::Spreadsheet);
        }
        let is_fourcc = (text.len() == 4)
            && text.chars().all(|ch| ch.is_ascii_alphanumeric());
        if is_fourcc && text.chars().any(|ch| ch.is_ascii_alphabetic()) {
            return Some(Artifact::Identifier);
        }
        None
    }

    /// Strict integer read (`0.5;ERC…` is an error).
    pub fn i32(&self, sheet: &str, row: &str, column: &str) -> Result<Option<i32>> {
        match self.text() {
            None => Ok(None),
            Some(text) => text.parse::<i32>().map(Some).map_err(|_| {
                GamedataError::BadCell {
                    sheet: sheet.to_string(),
                    row: row.to_string(),
                    column: column.to_string(),
                    value: text.to_string(),
                    expected: "an integer",
                }
            }),
        }
    }

    /// Integer read with engine semantics (leading number wins).
    pub fn i32_lossy(&self) -> Option<i32> {
        let value = self.leading_number()?;
        Some(value as i32)
    }

    /// Float read with engine semantics (leading number wins).
    pub fn f32_lossy(&self) -> Option<f32> {
        Some(self.leading_number()? as f32)
    }

    pub fn u32(&self, sheet: &str, row: &str, column: &str) -> Result<Option<u32>> {
        match self.i32(sheet, row, column)? {
            None => Ok(None),
            Some(v) if v >= 0 => Ok(Some(v as u32)),
            Some(v) => Err(GamedataError::BadCell {
                sheet: sheet.to_string(),
                row: row.to_string(),
                column: column.to_string(),
                value: v.to_string(),
                expected: "a non-negative integer",
            }),
        }
    }

    pub fn f32(&self, sheet: &str, row: &str, column: &str) -> Result<Option<f32>> {
        match self.text() {
            None => Ok(None),
            Some(text) => text.parse::<f32>().map(Some).map_err(|_| {
                GamedataError::BadCell {
                    sheet: sheet.to_string(),
                    row: row.to_string(),
                    column: column.to_string(),
                    value: text.to_string(),
                    expected: "a number",
                }
            }),
        }
    }

    /// WC3 booleans are `0`/`1`; `true`/`false` are accepted for robustness.
    pub fn bool(&self, sheet: &str, row: &str, column: &str) -> Result<Option<bool>> {
        match self.text() {
            None => Ok(None),
            Some("0") => Ok(Some(false)),
            Some("1") => Ok(Some(true)),
            Some(text) if text.eq_ignore_ascii_case("true") => Ok(Some(true)),
            Some(text) if text.eq_ignore_ascii_case("false") => Ok(Some(false)),
            Some(text) => Err(GamedataError::BadCell {
                sheet: sheet.to_string(),
                row: row.to_string(),
                column: column.to_string(),
                value: text.to_string(),
                expected: "a boolean (0/1)",
            }),
        }
    }
}

/// A sheet value: either a base SLK row or an object-data overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Present in the shipped SLK.
    Base,
    /// Written by an object-data file (`.w3u`, `.w3a`, …).
    Overlay,
}

/// Strict SLK sheet.
///
/// Differences from [`crate::slk::SlkTable`] (which stays untouched as the
/// low-level reader used elsewhere):
/// - column names are indexed case-insensitively, matching the game
///   (`slk.ixx:145`);
/// - values are read as the remainder of the line after `K`, so a quoted cell
///   may contain `;`;
/// - headerless rows are dropped and gaps closed (`slk.ixx:161-190`);
/// - the `B;` bounds record is cross-checked (`BoundsMismatch`);
/// - object-data overlay values live in a shadow layer that never mutates the
///   shipped numbers, so `Origin` can be reported per cell.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    name: String,
    columns: Vec<String>,
    column_index: HashMap<String, usize>,
    rows: Vec<Vec<String>>,
    row_ids: Vec<String>,
    row_index: HashMap<String, usize>,
    bounds: Option<SheetBounds>,
    shadow: HashMap<String, HashMap<String, String>>,
    shadow_columns: Vec<String>,
    overlay_cells: usize,
    actual_rows: usize,
    actual_columns: usize,
    skipped_rows: usize,
}

impl Sheet {
    /// Parse a sheet from raw SLK bytes.
    ///
    /// The shared [`crate::slk::SlkTable`] parser runs first as a well-formedness
    /// check (it must accept the file); the strict reader then produces the sheet
    /// used by the typed layer. Use [`Sheet::legacy_view`] to compare both.
    pub fn parse(name: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let name = name.into();
        let legacy = crate::slk::SlkTable::parse(bytes)?;
        if legacy.is_empty() && legacy.columns.is_empty() {
            return Err(GamedataError::Format(FormatError::Corrupt(
                "SLK has no cells",
            )));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| GamedataError::Format(FormatError::Corrupt("SLK is not UTF-8")))?;
        let mut sheet = Self {
            name: name.clone(),
            ..Self::default()
        };
        sheet.read(text);
        sheet.validate_bounds()?;
        if sheet.columns.is_empty() {
            return Err(GamedataError::MissingColumn {
                sheet: name,
                column: "<header row>".to_string(),
            });
        }
        Ok(sheet)
    }

    /// Independent view produced by [`crate::slk::SlkTable`], for cross-checks.
    pub fn legacy_view(bytes: &[u8]) -> std::result::Result<(Vec<String>, usize), FormatError> {
        let table = crate::slk::SlkTable::parse(bytes)?;
        let rows = table.len();
        Ok((table.columns, rows))
    }

    fn read(&mut self, text: &str) {
        // Sparse grid first: cells arrive in X/Y order but files may skip cells.
        let mut grid: Vec<Vec<Option<String>>> = Vec::new();
        // SLK streams are positional: a record may carry only `X` (same row) or
        // only `Y` (same column), so the cursor persists between records
        // (`donors/HiveWE/src/file_formats/slk.ixx:70-104`).
        let mut column = 0usize;
        let mut row = 0usize;
        // Last column written in the current row; drives the "column went
        // backwards means new row" rule for Y-less sheets.
        let mut previous_written: Option<usize> = None;
        // True when the current record (or the `F` record before it) set an
        // explicit position, so a following value record writes *at* it instead
        // of continuing sequentially.
        let mut positioned = false;
        // Slots the dense grid has materialised so far (WC3-SLK-001).
        let mut cells_allocated = 0usize;

        for line in text.split('\n') {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            match line.as_bytes()[0] {
                b'B' => {
                    if let Some(bounds) = parse_bounds(line) {
                        self.bounds = Some(bounds);
                    }
                }
                b'C' | b'F' => {
                    // `F` records are formatting/position records: they move the
                    // cursor but carry no value. The shipped sheets rely on this
                    // (`UnitData.slk` writes `F;Y67;X1` then `C;K"Emoo"`, and
                    // `DestructableData.slk` writes `F;P0;FG0C;X22` before every
                    // cell), so both record kinds share the positional handler
                    // (`donors/HiveWE/src/file_formats/slk.ixx:70-152`).
                    let (x, y, value) = parse_position_line(line);
                    if x.is_some() || y.is_some() {
                        positioned = true;
                    }
                    if let Some(y) = y {
                        // An explicit row coordinate always starts a new row.
                        row = y.saturating_sub(1);
                        previous_written = None;
                    }
                    if let Some(x) = x {
                        let next = x.saturating_sub(1);
                        // Sheets written without any `Y` start a new row when the
                        // column goes backwards (`DestructableData.slk`: 5 800
                        // `F;…;X<n>` + `C;K…` pairs and not one `Y`).
                        if let Some(previous) = previous_written {
                            if next < previous {
                                row += 1;
                                previous_written = None;
                            }
                        }
                        column = next;
                    } else if value.is_some() && !positioned {
                        // A value record with no position continues the row in the
                        // next column; when an `F`/`X` record positioned the cursor
                        // first, the value belongs *at* that position
                        // (`EnvironmentSounds.slk` writes `F;…;X3` then `C;K1`).
                        if let Some(previous) = previous_written {
                            column = previous + 1;
                        }
                    }
                    let Some(value) = value else { continue };
                    if row >= MAX_SHEET_ROWS || column >= MAX_SHEET_COLUMNS {
                        // Forged/spilled position: skip the element and report,
                        // never grow a grid to the declared coordinate.
                        crate::diag::note("slk_position_out_of_range", || {
                            format!(
                                "sheet={} row={row} column={column} limit={MAX_SHEET_ROWS}x{MAX_SHEET_COLUMNS}",
                                self.name
                            )
                        });
                        continue;
                    }
                    // The product of the per-axis limits is what used to
                    // amplify a single forged coordinate into a 67 M-slot grid.
                    let grown = column
                        .saturating_add(1)
                        .saturating_sub(grid.get(row).map_or(0, Vec::len));
                    if cells_allocated.saturating_add(grown) > MAX_SHEET_CELLS {
                        crate::diag::note("slk_grid_too_large", || {
                            format!(
                                "sheet={} cells={} limit={MAX_SHEET_CELLS}",
                                self.name, cells_allocated
                            )
                        });
                        continue;
                    }
                    cells_allocated += grown;
                    if grid.len() <= row {
                        grid.resize_with(row + 1, Vec::new);
                    }
                    let cells = &mut grid[row];
                    if cells.len() <= column {
                        cells.resize_with(column + 1, || None);
                    }
                    cells[column] = Some(value);
                    previous_written = Some(column);
                    positioned = false;
                }
                _ => {}
            }
        }

        // Width comes from the cells themselves: a few sheets declare a wider
        // `B;X…` than they use (`UI\SoundInfo\EAXDefs.slk` declares 6 columns
        // and fills 5), and padding with empty headers would invent columns.
        let width = grid.iter().map(Vec::len).max().unwrap_or(0);
        self.actual_rows = grid.len();
        self.actual_columns = width;

        // Column headers: row 0, lowercased for lookup, original text preserved.
        if let Some(header) = grid.first() {
            for index in 0..width {
                let raw = header
                    .get(index)
                    .and_then(|cell| cell.as_deref())
                    .unwrap_or("")
                    .to_string();
                self.columns.push(raw.clone());
                if !raw.is_empty() {
                    let key = raw.to_ascii_lowercase();
                    self.column_index.entry(key).or_insert(index);
                }
            }
        }

        // Row headers: column 0; headerless rows are dropped (HiveWE `slk.ixx:161`).
        for (index, cells) in grid.iter().enumerate().skip(1) {
            let id = cells
                .first()
                .and_then(|cell| cell.as_deref())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                // Positional row without an identifier: dropped, but counted.
                self.skipped_rows += 1;
                continue;
            }
            let mut values = Vec::with_capacity(width);
            for column in 0..width {
                values.push(
                    cells
                        .get(column)
                        .and_then(|cell| cell.clone())
                        .unwrap_or_default(),
                );
            }
            if values.len() > 1 {
                // Keep the id cell in place; downstream code indexes columns
                // exactly as the header row does.
                self.row_ids.push(id.clone());
                self.row_index.entry(id.clone()).or_insert(self.rows.len());
                self.rows.push(values);
            } else {
                // Single-column sheet: the id is the only value.
                self.row_ids.push(id.clone());
                self.row_index.entry(id.clone()).or_insert(self.rows.len());
                self.rows.push(values);
            }
            let _ = index;
        }
    }

    fn validate_bounds(&self) -> Result<()> {
        let Some(bounds) = self.bounds else {
            return Ok(());
        };
        // `B;Y..;X..` is an upper bound, not an exact size: several shipped
        // sheets declare a larger grid than they fill
        // (`UI\SoundInfo\EnvironmentSounds.slk` declares 13 rows and writes 5).
        if bounds.rows == 0 || bounds.columns == 0 {
            return Ok(());
        }
        if self.actual_rows > bounds.rows || self.actual_columns > bounds.columns {
            return Err(GamedataError::BoundsMismatch {
                sheet: self.name.clone(),
                declared_rows: bounds.rows,
                declared_columns: bounds.columns,
                parsed_rows: self.actual_rows,
                parsed_columns: self.actual_columns,
            });
        }
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bounds(&self) -> Option<SheetBounds> {
        self.bounds
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// Extra columns created by object-data overlays (not present in the SLK).
    pub fn shadow_columns(&self) -> &[String] {
        &self.shadow_columns
    }

    /// Number of cells written by overlays.
    pub fn overlay_cells(&self) -> usize {
        self.overlay_cells
    }

    /// Positional rows dropped because they carry no identifier (spacer rows).
    pub fn skipped_rows(&self) -> usize {
        self.skipped_rows
    }

    /// Rows declared by the `B;` record, header included.
    pub fn declared_rows(&self) -> Option<usize> {
        self.bounds.map(|bounds| bounds.rows)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row_ids(&self) -> &[String] {
        &self.row_ids
    }

    pub fn column_index(&self, column: &str) -> Option<usize> {
        self.column_index.get(&column.to_ascii_lowercase()).copied()
    }

    /// Row index of an object id. FourCC identifiers are case sensitive
    /// (`hfoo` and `HFOO` are different ids), so no case folding happens here.
    pub fn id_index(&self, id: &str) -> Option<usize> {
        self.row_index.get(id).copied()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.id_index(id).is_some()
    }

    pub fn row_id(&self, row: usize) -> Option<&str> {
        self.row_ids.get(row).map(String::as_str)
    }

    pub fn row(&self, row: usize) -> Option<&[String]> {
        self.rows.get(row).map(Vec::as_slice)
    }

    pub fn rows(&self) -> impl Iterator<Item = &[String]> {
        self.rows.iter().map(Vec::as_slice)
    }

    /// Cell by row index and column index.
    pub fn raw_cell(&self, row: usize, column: usize) -> Option<Cell<'_>> {
        let value = self.rows.get(row)?.get(column)?;
        Some(Cell::new(value))
    }

    /// Cell by row id and column name (case-insensitive).
    pub fn cell(&self, id: &str, column: &str) -> Option<Cell<'_>> {
        let column = self.column_index(column)?;
        let row = self.id_index(id)?;
        self.raw_cell(row, column)
    }

    /// Cell including overlay shadow values.
    pub fn cell_with_overlay(&self, id: &str, column: &str) -> Option<(Cell<'_>, Origin)> {
        let key = column.to_ascii_lowercase();
        if let Some(map) = self.shadow.get(id) {
            if let Some(value) = map.get(&key) {
                // Leak-free: shadow strings live in `self`, so a `Cell` borrow is
                // valid for the sheet's lifetime.
                return Some((Cell::new(value.as_str()), Origin::Overlay));
            }
        }
        self.cell(id, column).map(|cell| (cell, Origin::Base))
    }

    /// Write an overlay value. Columns missing from the SLK become shadow
    /// columns (HiveWE's `set_shadow_data`).
    pub fn set_overlay(&mut self, id: &str, column: &str, value: &str) {
        let key = column.to_ascii_lowercase();
        if !self.column_index.contains_key(&key) && !self.shadow_columns.contains(&key) {
            self.shadow_columns.push(key.clone());
        }
        let entry = self.shadow.entry(id.to_string()).or_default();
        if entry.insert(key, value.to_string()).is_none() {
            self.overlay_cells += 1;
        }
    }

    /// Insert or replace a whole row. Used when object data creates a custom
    /// object based on an existing one.
    pub fn upsert_row(&mut self, id: &str, values: Vec<String>) -> usize {
        let mut values = values;
        values.resize(self.columns.len(), String::new());
        if let Some(index) = self.id_index(id) {
            self.rows[index] = values;
            return index;
        }
        let index = self.rows.len();
        self.row_ids.push(id.to_string());
        self.row_index.insert(id.to_string(), index);
        self.rows.push(values);
        index
    }

    /// Copy `from` to `to`, preserving every cell. Returns `None` when the
    /// source row does not exist.
    pub fn clone_row(&mut self, from: &str, to: &str) -> Option<usize> {
        let source = self.id_index(from)?;
        let mut values = self.rows[source].clone();
        if let Some(first) = values.first_mut() {
            *first = to.to_string();
        }
        Some(self.upsert_row(to, values))
    }
}

fn parse_bounds(line: &str) -> Option<SheetBounds> {
    let mut rows = None;
    let mut columns = None;
    for field in line.split(';') {
        if let Some(rest) = field.strip_prefix('Y') {
            rows = rest.trim().parse::<usize>().ok();
        } else if let Some(rest) = field.strip_prefix('X') {
            columns = rest.trim().parse::<usize>().ok();
        }
    }
    match (rows, columns) {
        (Some(rows), Some(columns)) => Some(SheetBounds { rows, columns }),
        _ => None,
    }
}

/// Parse one `C;X…;Y…;K…` or `F;…;X…` record.
///
/// Returns `(x, y, value)`; `x`/`y` are 1-based when present. Every field is
/// scanned, because format records interleave other fields with the position
/// (`F;P0;FG0C;X22`). The value is the remainder of the line after the `K`
/// marker, which keeps `;` inside quoted cells (HiveWE reads the rest of the
/// line, `slk.ixx:109-117`).
fn parse_position_line(line: &str) -> (Option<usize>, Option<usize>, Option<String>) {
    let body = line.get(2..).unwrap_or("");
    let mut x = None;
    let mut y = None;
    let mut rest = body;
    loop {
        rest = rest.trim_start_matches(';');
        if rest.is_empty() {
            return (x, y, None);
        }
        let mut chars = rest.chars();
        let Some(first) = chars.next() else {
            return (x, y, None);
        };
        let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
        match first {
            'X' if !digits.is_empty() => {
                x = digits.parse().ok();
                rest = &rest[1 + digits.len()..];
            }
            'Y' if !digits.is_empty() => {
                y = digits.parse().ok();
                rest = &rest[1 + digits.len()..];
            }
            'K' => {
                return (x, y, Some(unquote(&rest[1..])));
            }
            _ => {
                // Skip to the next field separator.
                match rest.find(';') {
                    Some(position) => rest = &rest[position + 1..],
                    None => return (x, y, None),
                }
            }
        }
    }
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim_end_matches(['\r']);
    if let Some(inner) = trimmed.strip_prefix('"') {
        return inner.strip_suffix('"').unwrap_or(inner).to_string();
    }
    trimmed.to_string()
}

/// Field descriptor kind, selecting which metadata sheet a `MetaTable` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetaKind {
    Unit,
    Ability,
    Item,
    Upgrade,
    Destructable,
    Doodad,
    Buff,
    Misc,
    Skin,
    UpgradeEffect,
}

impl MetaKind {
    pub fn sheet_name(&self) -> &'static str {
        match self {
            Self::Unit => "UnitMetaData.slk",
            Self::Ability => "AbilityMetaData.slk",
            Self::Item => "ItemMetaData.slk",
            Self::Upgrade => "UpgradeMetaData.slk",
            Self::Destructable => "DestructableMetaData.slk",
            Self::Doodad => "DoodadMetaData.slk",
            Self::Buff => "AbilityBuffMetaData.slk",
            Self::Misc => "MiscMetaData.slk",
            Self::Skin => "SkinMetaData.slk",
            Self::UpgradeEffect => "UpgradeEffectMetaData.slk",
        }
    }

    /// Metadata sheet used for items; RoC/TFT ship item fields inside
    /// `UnitMetaData.slk` (rows with `useItem=1`), so there is no
    /// `ItemMetaData.slk` in the retail data (`observed`: absent from both
    /// `analysis/war3_data/Units` and `analysis/war3x_data/Units`).
    pub fn item_fallback(&self) -> Option<MetaKind> {
        matches!(self, Self::Item).then_some(MetaKind::Unit)
    }
}

/// One field descriptor row from a `*MetaData.slk` sheet.
///
/// Unknown columns stay reachable through [`MetaField::raw_column`]; nothing is
/// given a name the shipped data does not use.
#[derive(Debug, Clone, Copy)]
pub struct MetaField<'a> {
    sheet: &'a Sheet,
    row: usize,
}

impl<'a> MetaField<'a> {
    pub fn sheet(&self) -> &'a Sheet {
        self.sheet
    }

    pub fn row(&self) -> usize {
        self.row
    }

    fn text(&self, column: &str) -> Option<&'a str> {
        let index = self.sheet.column_index(column)?;
        self.sheet.rows.get(self.row)?.get(index).map(String::as_str)
    }

    /// Verbatim value of any column of the metadata sheet.
    pub fn raw_column(&self, column: &str) -> Option<&'a str> {
        self.text(column)
    }

    pub fn id(&self) -> &'a str {
        self.text("ID").unwrap_or("")
    }

    pub fn field(&self) -> &'a str {
        self.text("field").unwrap_or("")
    }

    /// Target sheet named by the metadata (`Profile`, `UnitBalance`, …).
    pub fn slk(&self) -> &'a str {
        self.text("slk").unwrap_or("")
    }

    pub fn index(&self) -> i32 {
        self.text("index")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(-1)
    }

    /// `repeat` column: 0/absent means "not per-level".
    pub fn repeat(&self) -> i32 {
        self.text("repeat")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    }

    /// `data` column: editor slot index (1-based `DataA`..`DataE`).
    pub fn data(&self) -> i32 {
        self.text("data")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    }

    pub fn type_name(&self) -> &'a str {
        self.text("type").unwrap_or("")
    }

    pub fn display_name(&self) -> &'a str {
        self.text("displayName").unwrap_or("")
    }

    pub fn sort(&self) -> &'a str {
        self.text("sort").unwrap_or("")
    }

    pub fn category(&self) -> &'a str {
        self.text("category").unwrap_or("")
    }

    /// True when the field lives in the profile text layer (`UnitStrings.txt`,
    /// `*UnitFunc.txt`, …) instead of a SLK column.
    pub fn is_profile(&self) -> bool {
        self.slk().eq_ignore_ascii_case("Profile")
    }

    /// Column name that a modification of this field writes, following
    /// HiveWE's rule (`modification_tables.ixx:39-53`):
    /// `lower(field)` + optional data-slot letter (`a`+pointer-1) + optional
    /// level suffix when `repeat != 0`.
    pub fn column_for(&self, level: u32, data_pointer: u32) -> String {
        let mut column = self.field().to_ascii_lowercase();
        if data_pointer != 0 {
            column.push((b'a' + (data_pointer.min(26) as u8 - 1)) as char);
        }
        if self.repeat() != 0 && level != 0 {
            column.push_str(&level.to_string());
        }
        column
    }
}

/// `*MetaData.slk` accessor.
#[derive(Debug, Clone)]
pub struct MetaTable {
    kind: MetaKind,
    sheet: Sheet,
    by_id: HashMap<String, usize>,
}

impl MetaTable {
    pub fn parse(kind: MetaKind, bytes: &[u8]) -> Result<Self> {
        let sheet = Sheet::parse(kind.sheet_name(), bytes)?;
        let mut by_id = HashMap::with_capacity(sheet.len());
        for (row, id) in sheet.row_ids().iter().enumerate() {
            by_id.entry(id.clone()).or_insert(row);
        }
        Ok(Self { kind, sheet, by_id })
    }

    pub fn kind(&self) -> MetaKind {
        self.kind
    }

    pub fn len(&self) -> usize {
        self.sheet.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sheet.is_empty()
    }

    pub fn sheet(&self) -> &Sheet {
        &self.sheet
    }

    pub fn get(&self, id: &str) -> Option<MetaField<'_>> {
        let row = *self.by_id.get(id)?;
        Some(MetaField {
            sheet: &self.sheet,
            row,
        })
    }

    pub fn fields(&self) -> impl Iterator<Item = MetaField<'_>> {
        (0..self.sheet.len()).map(move |row| MetaField {
            sheet: &self.sheet,
            row,
        })
    }

    /// Fields whose `slk` names `sheet_name`.
    pub fn fields_for_sheet<'b>(
        &'b self,
        sheet_name: &'b str,
    ) -> impl Iterator<Item = MetaField<'b>> + 'b {
        self.fields()
            .filter(move |field| field.slk().eq_ignore_ascii_case(sheet_name))
    }
}

/// Object-data file kinds and their on-disk extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Unit,
    Ability,
    Item,
    Upgrade,
    Buff,
    Destructable,
    Doodad,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 7] = [
        ObjectKind::Unit,
        ObjectKind::Ability,
        ObjectKind::Item,
        ObjectKind::Upgrade,
        ObjectKind::Buff,
        ObjectKind::Destructable,
        ObjectKind::Doodad,
    ];

    /// Map file extension without the dot (`u` for `war3map.w3u`).
    ///
    /// Verified against HiveWE's loader (`donors/HiveWE/src/base/map/map.ixx`
    /// 430-492): `w3d` = doodads, `w3b` = destructables, `w3h` = buffs. The
    /// earlier table had those three swapped.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Unit => "w3u",
            Self::Ability => "w3a",
            Self::Item => "w3t",
            Self::Upgrade => "w3q",
            Self::Buff => "w3h",
            Self::Destructable => "w3b",
            Self::Doodad => "w3d",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.extension().eq_ignore_ascii_case(ext))
    }

    pub fn meta_kind(&self) -> MetaKind {
        match self {
            Self::Unit => MetaKind::Unit,
            Self::Ability => MetaKind::Ability,
            Self::Item => MetaKind::Item,
            Self::Upgrade => MetaKind::Upgrade,
            Self::Buff => MetaKind::Buff,
            Self::Destructable => MetaKind::Destructable,
            Self::Doodad => MetaKind::Doodad,
        }
    }

    /// True when each modification carries `level`/`variation` + `data pointer`
    /// fields. `observed`: `extended()` returns false only for unit/buff/item/
    /// destructable (`War3ObjectDataChangeset.java:230-245`).
    pub fn extended(&self) -> bool {
        matches!(self, Self::Ability | Self::Upgrade | Self::Doodad)
    }
}

/// Typed object-data value.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeValue {
    Int(i32),
    Real(f32),
    Unreal(f32),
    String(String),
    Bool(bool),
    /// Obsolete `char` type (code 5); kept so old files parse without loss.
    Char(i32),
}

impl ChangeValue {
    pub fn as_text(&self) -> String {
        match self {
            Self::Int(v) => v.to_string(),
            Self::Real(v) | Self::Unreal(v) => format_float(*v),
            Self::String(v) => v.clone(),
            Self::Bool(v) => if *v { "1" } else { "0" }.to_string(),
            Self::Char(v) => v.to_string(),
        }
    }

    pub fn type_code(&self) -> u32 {
        match self {
            Self::Int(_) => 0,
            Self::Real(_) => 1,
            Self::Unreal(_) => 2,
            Self::String(_) => 3,
            Self::Bool(_) => 4,
            Self::Char(_) => 5,
        }
    }
}

/// WC3 prints reals without trailing `.0` noise; SLK cells keep the shortest
/// round-trip form so overlay values compare equal to shipped cells.
fn format_float(value: f32) -> String {
    let mut text = format!("{value}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

/// One field modification inside an object entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectChange {
    pub id: String,
    pub value: ChangeValue,
    /// Ability/upgrade level or doodad variation (0 for simple kinds).
    pub level: u32,
    /// Ability data slot pointer (1-based `DataA`…); 0 when unused.
    pub data_pointer: u32,
}

/// One object entry: a modification of a shipped object or a custom object.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectEntry {
    /// Base object; all-zero for entries in the custom section.
    pub old_id: FourCc,
    /// New object id; all-zero for modifications of a shipped object.
    pub new_id: FourCc,
    pub changes: Vec<ObjectChange>,
}

impl ObjectEntry {
    /// Identifier the entry is stored under (new id for custom objects).
    pub fn target_id(&self) -> FourCc {
        if self.new_id.is_zero() {
            self.old_id
        } else {
            self.new_id
        }
    }

    pub fn is_custom(&self) -> bool {
        !self.new_id.is_zero()
    }
}

/// Parsed object-data file (`.w3u`, `.w3a`, `.w3t`, `.w3q`, `.w3b`, `.w3d`, `.w3h`).
#[derive(Debug, Clone)]
pub struct ObjectDataFile {
    pub kind: ObjectKind,
    pub version: u32,
    /// Section 1: modifications of shipped objects.
    pub base: Vec<ObjectEntry>,
    /// Section 2: custom objects (`old_id` = prototype, `new_id` = new id).
    pub custom: Vec<ObjectEntry>,
}

/// Corruption guards. Retail files are small; a bogus count must fail loudly
/// instead of allocating gigabytes.
#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    pub max_entries: usize,
    pub max_changes: usize,
    pub max_string: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_changes: 1_000_000,
            max_string: 1 << 20,
        }
    }
}

impl ObjectDataFile {
    /// Parse a per-kind object-data file.
    pub fn parse(kind: ObjectKind, bytes: &[u8]) -> Result<Self> {
        Self::parse_with_limits(kind, bytes, ParseLimits::default())
    }

    pub fn parse_with_limits(kind: ObjectKind, bytes: &[u8], limits: ParseLimits) -> Result<Self> {
        Self::parse_ex_with_limits(kind, bytes, limits).map(|(file, _)| file)
    }

    /// Parse a per-kind object-data file and report the number of bytes
    /// consumed. A well-formed file is consumed exactly; anything left over
    /// means the record layout drifted.
    pub fn parse_ex(kind: ObjectKind, bytes: &[u8]) -> Result<(Self, usize)> {
        Self::parse_ex_with_limits(kind, bytes, ParseLimits::default())
    }

    pub fn parse_ex_with_limits(
        kind: ObjectKind,
        bytes: &[u8],
        limits: ParseLimits,
    ) -> Result<(Self, usize)> {
        let mut reader = Reader::new(bytes, limits);
        let version = reader.u32()?;
        if !(1..=3).contains(&version) {
            return Err(GamedataError::UnsupportedVersion {
                file: kind.extension(),
                version,
            });
        }
        let read_section = |reader: &mut Reader<'_>| -> Result<Vec<ObjectEntry>> {
            let count = reader.u32()? as usize;
            if count > limits.max_entries {
                return Err(GamedataError::CountLimit {
                    what: "object entries",
                    count,
                    limit: limits.max_entries,
                });
            }
            let mut entries = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                entries.push(reader.entry(kind, version)?);
            }
            Ok(entries)
        };
        let base = read_section(&mut reader)?;
        let custom = read_section(&mut reader)?;
        let consumed = reader.pos;
        Ok((
            Self {
                kind,
                version,
                base,
                custom,
            },
            consumed,
        ))
    }

    /// Entries of both sections, base first.
    pub fn entries(&self) -> impl Iterator<Item = &ObjectEntry> {
        self.base.iter().chain(self.custom.iter())
    }

    pub fn len(&self) -> usize {
        self.base.len() + self.custom.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    limits: ParseLimits,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], limits: ParseLimits) -> Self {
        Self {
            data,
            pos: 0,
            limits,
        }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(GamedataError::Format(FormatError::Corrupt(
                "object offset overflow",
            )))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or(GamedataError::Format(FormatError::Corrupt(
                "object read past end",
            )))?;
        self.pos = end;
        Ok(slice)
    }

    /// Read exactly `N` bytes as an array (`first_chunk` cannot lie).
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        let slice = self.bytes(N)?;
        slice
            .first_chunk::<N>()
            .copied()
            .ok_or(GamedataError::Format(FormatError::Corrupt(
                "object short read",
            )))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr::<4>()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.arr::<4>()?))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.arr::<4>()?))
    }

    fn fourcc(&mut self) -> Result<FourCc> {
        Ok(FourCc::new(self.arr::<4>()?))
    }

    fn c_string(&mut self) -> Result<String> {
        let start = self.pos;
        // `get`-based: a truncated file (pos == len) returns the typed error
        // instead of slicing an index that does not exist.
        let tail = self
            .data
            .get(start..)
            .ok_or(GamedataError::Format(FormatError::Corrupt(
                "object string past end",
            )))?;
        let nul = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or(GamedataError::Format(FormatError::Corrupt(
                "unterminated object string",
            )))?;
        if nul > self.limits.max_string {
            return Err(GamedataError::CountLimit {
                what: "object string bytes",
                count: nul,
                limit: self.limits.max_string,
            });
        }
        let end = start.checked_add(nul).ok_or(GamedataError::Format(
            FormatError::Corrupt("object string overflow"),
        ))?;
        let text = String::from_utf8_lossy(&tail[..nul]).into_owned();
        self.pos = end + 1;
        Ok(text)
    }

    fn entry(&mut self, kind: ObjectKind, version: u32) -> Result<ObjectEntry> {
        let old_id = self.fourcc()?;
        let new_id = self.fourcc()?;
        if version >= 3 {
            // 1.33+ writes a set list; HiveWE calls it "sets count/flags"
            // (`modification_tables.ixx:24-27`), Warsmash "reforged junk".
            let sets = self.u32()? as usize;
            if sets > self.limits.max_entries {
                return Err(GamedataError::CountLimit {
                    what: "object sets",
                    count: sets,
                    limit: self.limits.max_entries,
                });
            }
            for _ in 0..sets {
                self.u32()?;
            }
        }
        let count = self.u32()? as usize;
        if count > self.limits.max_changes {
            return Err(GamedataError::CountLimit {
                what: "object changes",
                count,
                limit: self.limits.max_changes,
            });
        }
        let mut changes = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let id = self.fourcc()?.as_str().into_owned();
            let type_code = self.u32()?;
            let (level, data_pointer) = if kind.extended() {
                (self.u32()?, self.u32()?)
            } else {
                (0, 0)
            };
            let value = match type_code {
                0 => ChangeValue::Int(self.i32()?),
                1 => ChangeValue::Real(self.f32()?),
                2 => ChangeValue::Unreal(self.f32()?),
                3 => ChangeValue::String(self.c_string()?),
                4 => ChangeValue::Bool(self.i32()? != 0),
                5 => ChangeValue::Char(self.i32()?),
                other => {
                    return Err(GamedataError::Format(FormatError::Unsupported(
                        object_value_type_name(other),
                    )));
                }
            };
            // Trailing "sanity" word: always 0 in every shipped file, and
            // Warsmash reads it as a 4-byte extra (`War3ObjectDataChangeset.java:588`).
            let _sanity = self.u32()?;
            changes.push(ObjectChange {
                id,
                value,
                level,
                data_pointer,
            });
        }
        Ok(ObjectEntry {
            old_id,
            new_id,
            changes,
        })
    }
}

fn object_value_type_name(code: u32) -> &'static str {
    match code {
        6..=u32::MAX => "object data value type",
        _ => "object data value type",
    }
}

/// Resolution outcome for a modification field id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTarget {
    /// Field writes a SLK column of the named sheet.
    Column {
        sheet: String,
        column: String,
        present: bool,
    },
    /// Field writes the profile text layer (key + list index).
    Profile { key: String, index: i32 },
    /// Field id is not described by any metadata row.
    Unknown,
}

/// Resolve where a modification lands.
pub fn resolve_field(
    meta: Option<&MetaTable>,
    field_id: &str,
    level: u32,
    data_pointer: u32,
) -> FieldTarget {
    let Some(field) = meta.and_then(|meta| meta.get(field_id)) else {
        return FieldTarget::Unknown;
    };
    if field.is_profile() {
        return FieldTarget::Profile {
            key: field.field().to_string(),
            index: field.index(),
        };
    }
    let column = field.column_for(level, data_pointer);
    FieldTarget::Column {
        sheet: field.slk().to_string(),
        present: false,
        column,
    }
}

/// Profile text overlay written by object data (`unam`, `utip`, `aart`, …).
///
/// Values are stored per key and per comma-separated component, mirroring the
/// `index` column of the metadata sheets (`ua2m|Missileart|Profile|1` addresses
/// the second entry of `Missileart=a.mdl,b.mdl`).
#[derive(Debug, Clone, Default)]
pub struct ProfileOverlay {
    sections: HashMap<String, HashMap<String, Vec<String>>>,
}

impl ProfileOverlay {
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    pub fn sections(&self) -> impl Iterator<Item = (&String, &HashMap<String, Vec<String>>)> {
        self.sections.iter()
    }

    /// Set component `index` of `key` for object `id`.
    pub fn set(&mut self, id: &str, key: &str, index: i32, value: &str) {
        let index = if index < 0 { 0 } else { index as usize };
        // Level/data-pointer components are bounded by the `levels` field of
        // the object; a forged index must not grow a vector without limit.
        if index >= MAX_PROFILE_COMPONENTS {
            crate::diag::note("profile_component_index", || {
                format!("id={id} key={key} index={index} limit={MAX_PROFILE_COMPONENTS}")
            });
            return;
        }
        let components = self
            .sections
            .entry(id.to_string())
            .or_default()
            .entry(key.to_ascii_lowercase())
            .or_default();
        while components.len() <= index {
            components.push(String::new());
        }
        components[index] = value.to_string();
    }

    pub fn get(&self, id: &str, key: &str, index: usize) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        self.sections
            .get(id)?
            .get(&key)?
            .get(index)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// Joined value of `key` (`a,b`).
    pub fn joined(&self, id: &str, key: &str) -> Option<String> {
        let key = key.to_ascii_lowercase();
        let components = self.sections.get(id)?.get(&key)?;
        if components.is_empty() {
            return None;
        }
        Some(components.join(","))
    }
}

/// Per-field application outcome counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Modifications written to a column or profile key.
    pub applied: usize,
    /// Modification written into the shadow layer (column absent from the SLK).
    pub shadow: usize,
    /// Modification written to the profile text layer.
    pub profile: usize,
    /// Modification whose field id has no metadata row.
    pub unknown_field: Vec<(String, String)>,
    /// Modification targeting a sheet that was not supplied to the merge.
    pub missing_sheet: Vec<(String, String)>,
    /// Custom entries created per sheet.
    pub created: Vec<(String, String)>,
    /// Custom entries whose prototype row is absent from a sheet.
    pub prototype_missing: Vec<(String, String)>,
}

/// Target sheet supplied to [`merge_overlay`].
pub struct MergeTarget<'a> {
    pub name: &'a str,
    pub sheet: &'a mut Sheet,
    /// Whether custom objects clone their prototype row into this sheet.
    ///
    /// Unit object data may also touch `ItemData` (the `isst` field edits item
    /// stock), but a custom *unit* must not be created as an item row.
    pub create_rows: bool,
}

/// Apply an object-data file over the shipped sheets plus the profile overlay.
///
/// Mirrors the reference behaviour:
/// - custom entries clone their prototype row into every supplied sheet that
///   contains it (`HiveWE modification_tables.ixx:28-31`);
/// - modifications address `lower(field)` + data-slot letter + level suffix;
/// - unknown field ids are reported, never guessed (`modification_tables.ixx:73-77`).
pub fn merge_overlay(
    kind: ObjectKind,
    file: &ObjectDataFile,
    meta: Option<&MetaTable>,
    targets: &mut [MergeTarget<'_>],
    profile: &mut ProfileOverlay,
) -> MergeReport {
    let mut report = MergeReport::default();
    debug_assert_eq!(kind, file.kind, "overlay kind must match its file");

    for entry in file.entries() {
        let is_custom = entry.is_custom() && !entry.old_id.is_zero();
        let target_id = entry.target_id().as_str().into_owned();

        if is_custom {
            for target in targets.iter_mut().filter(|target| target.create_rows) {
                if !target.sheet.contains(&target_id) {
                    if target.sheet.contains(&entry.old_id.as_str()) {
                        target.sheet.clone_row(&entry.old_id.as_str(), &target_id);
                        report.created.push((target.name.to_string(), target_id.clone()));
                    } else {
                        report
                            .prototype_missing
                            .push((target.name.to_string(), target_id.clone()));
                    }
                }
            }
        }

        for change in &entry.changes {
            if change.id.is_empty() {
                continue;
            }
            match resolve_field(meta, &change.id, change.level, change.data_pointer) {
                FieldTarget::Profile { key, index } => {
                    profile.set(&target_id, &key, index, &change.value.as_text());
                    report.profile += 1;
                }
                FieldTarget::Column { sheet, column, .. } => {
                    let Some(target) = targets
                        .iter_mut()
                        .find(|target| target.name.eq_ignore_ascii_case(&sheet))
                    else {
                        report
                            .missing_sheet
                            .push((sheet.clone(), change.id.clone()));
                        continue;
                    };
                    let present = target.sheet.column_index(&column).is_some();
                    target.sheet.set_overlay(&target_id, &column, &change.value.as_text());
                    if present {
                        report.applied += 1;
                    } else {
                        report.shadow += 1;
                    }
                }
                FieldTarget::Unknown => report
                    .unknown_field
                    .push(("?".to_string(), change.id.clone())),
            }
        }
    }
    report
}

/// `.wts` string table (`war3map.wts`).
///
/// Grammar: `STRING <n>` line, `{` line, arbitrary text lines, `}` line. A
/// UTF-8 BOM is skipped. Multi-line values are joined with `\n`
/// (`donors/WarsmashModEngine/.../units/custom/WTSFile.java:36-81`).
#[derive(Debug, Clone, Default)]
pub struct Wts {
    strings: HashMap<u32, String>,
}

impl Wts {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| GamedataError::Wts("file is not valid UTF-8".to_string()))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut strings = HashMap::new();
        let mut pending: Option<u32> = None;
        let mut in_block = false;
        let mut buffer = String::new();
        for line in text.split('\n') {
            let line = line.trim_end_matches('\r');
            if in_block {
                if line.trim_start().starts_with('}') {
                    if let Some(key) = pending.take() {
                        strings.insert(key, buffer.clone());
                    }
                    buffer.clear();
                    in_block = false;
                } else {
                    if !buffer.is_empty() {
                        buffer.push('\n');
                    }
                    buffer.push_str(line);
                }
                continue;
            }
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("STRING ") {
                let key = rest
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| GamedataError::Wts(format!("bad STRING key {rest:?}")))?;
                pending = Some(key);
            } else if trimmed.starts_with('{') && pending.is_some() {
                in_block = true;
                buffer.clear();
            } else if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
        }
        if in_block {
            return Err(GamedataError::Wts("unterminated block".to_string()));
        }
        Ok(Self { strings })
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Iterate `(id, text)` pairs. The map layer builds its ordered view and
    /// applies escape decoding on top of this canonical parse.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.strings
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
    }

    pub fn get(&self, key: u32) -> Option<&str> {
        self.strings.get(&key).map(String::as_str)
    }

    /// Resolve a `TRIGSTR_<n>` reference; other text is returned unchanged.
    pub fn resolve(&self, text: &str) -> String {
        match parse_trigstr(text) {
            Some(key) => self.get(key).map(str::to_string).unwrap_or_else(|| text.to_string()),
            None => text.to_string(),
        }
    }
}

/// Extract the number from `TRIGSTR_<n>` / `TRIGSTR_<n> ` references.
pub fn parse_trigstr(text: &str) -> Option<u32> {
    let trimmed = text.trim();
    let mut rest = trimmed.strip_prefix("TRIGSTR_")?;
    // Files in the wild write `TRIGSTR_087` and `TRIGSTR_87`; both mean 87.
    rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    rest.parse::<u32>().ok()
}

/// INI-style table (`UnitStrings.txt`, `MiscData.txt`, …) with ordered sections.
///
/// Unlike [`crate::ini::Ini`] this keeps key order and every duplicate key, so
/// validation can report duplicates instead of silently collapsing them.
#[derive(Debug, Clone, Default)]
pub struct TextTable {
    name: String,
    sections: Vec<(String, Vec<(String, String)>)>,
    index: HashMap<String, usize>,
}

impl TextTable {
    pub fn parse(name: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            GamedataError::Format(FormatError::Corrupt("text table is not UTF-8"))
        })?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut table = Self {
            name: name.into(),
            ..Self::default()
        };
        let mut current: String = "global".to_string();
        table.sections.push((current.clone(), Vec::new()));
        table.index.insert("global".to_string(), 0);
        for line in text.split('\n') {
            let line = line.trim_end_matches('\r').trim();
            if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                current = line[1..line.len() - 1].trim().to_string();
                if !table.index.contains_key(&current) {
                    table.index.insert(current.clone(), table.sections.len());
                    table.sections.push((current.clone(), Vec::new()));
                }
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                // Values are kept verbatim: profile strings may be quoted lists
                // (`Ubertip="a","b"`), and stripping quotes here would make the
                // comma separators ambiguous.
                let value = value.trim().to_string();
                // A `key=value` line before the first `[Section]` used to index
                // section 0 of an empty table. Keep the line under a synthetic
                // section instead of panicking on a malformed profile file.
                let position = match table.index.get(&current).copied() {
                    Some(position) => position,
                    None => {
                        crate::diag::note("profile_section_missing", || {
                            format!("file={} key={}", table.name, key.trim())
                        });
                        if current.is_empty() {
                            current = "GLOBAL".to_string();
                        }
                        table.index.insert(current.clone(), table.sections.len());
                        table.sections.push((current.clone(), Vec::new()));
                        table.sections.len() - 1
                    }
                };
                table.sections[position]
                    .1
                    .push((key.trim().to_string(), value));
            }
        }
        Ok(table)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sections(&self) -> &[(String, Vec<(String, String)>)] {
        &self.sections
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    pub fn key_count(&self) -> usize {
        self.sections.iter().map(|(_, keys)| keys.len()).sum()
    }

    /// All values for `key` in `section` (last write wins for lookups).
    pub fn values(&self, section: &str, key: &str) -> Vec<&str> {
        let Some(index) = self.index.get(section) else {
            return Vec::new();
        };
        self.sections[*index]
            .1
            .iter()
            .filter(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.values(section, key).last().copied()
    }

    /// Verbatim value (last write wins), quotes included.
    pub fn value(&self, section: &str, key: &str) -> Option<&str> {
        self.get(section, key)
    }

    /// Component `index` of a comma separated value (`Buttonpos=0,2`).
    pub fn component(&self, section: &str, key: &str, index: usize) -> Option<&str> {
        let joined = self.get(section, key)?;
        joined.split(',').map(str::trim).nth(index)
    }

    /// Sections in file order.
    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|(name, _)| name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC: &[u8] = b"ID;PWXL;N;E\nB;Y4;X3;D0 0 3 2\nF;W1 1 6\n\
C;Y1;X1;K\"unitID\"\nC;X2;K\"HP\"\nC;X3;K\"Name\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K420\nC;X3;K\"a;b\"\n\
C;Y3;X1;K\"hpea\"\nC;X2;K-\nC;X3;K_\n";

    #[test]
    fn parses_sheet_with_semicolon_inside_quotes() {
        let sheet = Sheet::parse("test", SYNTHETIC).expect("parse");
        assert_eq!(sheet.columns(), ["unitID", "HP", "Name"]);
        assert_eq!(sheet.len(), 2);
        assert_eq!(sheet.cell("hfoo", "hp").unwrap().raw(), "420");
        assert_eq!(sheet.cell("hfoo", "name").unwrap().raw(), "a;b");
        assert!(sheet.cell("hpea", "hp").unwrap().is_absent());
        assert_eq!(sheet.bounds(), Some(SheetBounds { rows: 4, columns: 3 }));
    }

    #[test]
    fn column_lookup_is_case_insensitive() {
        let sheet = Sheet::parse("test", SYNTHETIC).expect("parse");
        assert_eq!(sheet.column_index("UNITID"), Some(0));
        assert_eq!(sheet.cell("hfoo", "NAME").unwrap().text(), Some("a;b"));
    }

    #[test]
    fn overlay_writes_shadow_when_column_missing() {
        let mut sheet = Sheet::parse("test", SYNTHETIC).expect("parse");
        sheet.set_overlay("hfoo", "def", "5");
        assert_eq!(sheet.shadow_columns(), ["def"]);
        let (cell, origin) = sheet.cell_with_overlay("hfoo", "def").unwrap();
        assert_eq!(cell.raw(), "5");
        assert_eq!(origin, Origin::Overlay);
        let (base, origin) = sheet.cell_with_overlay("hfoo", "hp").unwrap();
        assert_eq!(base.raw(), "420");
        assert_eq!(origin, Origin::Base);
    }

    #[test]
    fn clone_row_copies_and_renames() {
        let mut sheet = Sheet::parse("test", SYNTHETIC).expect("parse");
        sheet.clone_row("hfoo", "uC01").expect("clone");
        assert_eq!(sheet.cell("uC01", "hp").unwrap().raw(), "420");
        assert_eq!(sheet.row_id(sheet.len() - 1), Some("uC01"));
    }

    #[test]
    fn bounds_overflow_is_an_error_but_sparse_sheets_parse() {
        // Data beyond the declared grid is a real inconsistency.
        let overflow = b"ID;PWXL;N;E\nB;Y2;X3;D0\nC;Y1;X1;K\"a\"\nC;Y3;X1;K\"b\"\n";
        match Sheet::parse("overflow", overflow) {
            Err(GamedataError::BoundsMismatch {
                declared_rows,
                parsed_rows,
                ..
            }) => {
                assert_eq!(declared_rows, 2);
                assert_eq!(parsed_rows, 3);
            }
            other => panic!("expected bounds mismatch, got {other:?}"),
        }
        // A sheet that declares more rows than it fills is normal.
        let sparse = b"ID;PWXL;N;E\nB;Y9;X3;D0\nC;Y1;X1;K\"a\"\nC;Y2;X1;K\"b\"\n";
        let sheet = Sheet::parse("sparse", sparse).expect("sparse sheet");
        assert_eq!(sheet.len(), 1);
        assert_eq!(sheet.declared_rows(), Some(9));
    }

    fn entry(id: &[u8; 4], value: ChangeValue, level: u32, data_pointer: u32) -> ObjectChange {
        ObjectChange {
            id: String::from_utf8_lossy(id).into_owned(),
            value,
            level,
            data_pointer,
        }
    }

    /// Write only the value payload and the trailing sanity word.
    fn push_value(out: &mut Vec<u8>, value: &ChangeValue) {
        {
            match value.type_code() {
                0 | 4 | 5 => out.extend_from_slice(&match value {
                    ChangeValue::Int(v) => v.to_le_bytes(),
                    ChangeValue::Bool(v) => (*v as i32).to_le_bytes(),
                    ChangeValue::Char(v) => v.to_le_bytes(),
                    _ => unreachable!(),
                }),
                1 | 2 => {
                    let v = match value {
                        ChangeValue::Real(v) | ChangeValue::Unreal(v) => *v,
                        _ => unreachable!(),
                    };
                    out.extend_from_slice(&v.to_le_bytes());
                }
                3 => {
                    let ChangeValue::String(text) = value else {
                        unreachable!()
                    };
                    out.extend_from_slice(text.as_bytes());
                    out.push(0);
                }
                _ => unreachable!(),
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    fn build_object_file(
        kind: ObjectKind,
        version: u32,
        base: &[(FourCc, FourCc, Vec<ObjectChange>)],
        custom: &[(FourCc, FourCc, Vec<ObjectChange>)],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&version.to_le_bytes());
        for section in [base, custom] {
            out.extend_from_slice(&(section.len() as u32).to_le_bytes());
            for (old_id, new_id, changes) in section {
                out.extend_from_slice(old_id.bytes());
                out.extend_from_slice(new_id.bytes());
                if version >= 3 {
                    out.extend_from_slice(&1u32.to_le_bytes());
                    out.extend_from_slice(&0u32.to_le_bytes());
                }
                out.extend_from_slice(&(changes.len() as u32).to_le_bytes());
                for change in changes {
                    out.extend_from_slice(change.id.as_bytes());
                    out.extend_from_slice(&change.value.type_code().to_le_bytes());
                    if kind.extended() {
                        out.extend_from_slice(&change.level.to_le_bytes());
                        out.extend_from_slice(&change.data_pointer.to_le_bytes());
                    }
                    push_value(&mut out, &change.value);
                }
            }
        }
        out
    }

    #[test]
    fn object_file_roundtrip_v2_simple_and_custom() {
        let base = vec![(
            FourCc::parse("hfoo").unwrap(),
            FourCc::new([0; 4]),
            vec![entry(b"uhpm", ChangeValue::Int(420), 0, 0)],
        )];
        let custom = vec![(
            FourCc::parse("hfoo").unwrap(),
            FourCc::parse("h001").unwrap(),
            vec![
                entry(b"unam", ChangeValue::String("Custom".into()), 0, 0),
                entry(b"umvs", ChangeValue::Int(300), 0, 0),
            ],
        )];
        let bytes = build_object_file(ObjectKind::Unit, 2, &base, &custom);
        let file = ObjectDataFile::parse(ObjectKind::Unit, &bytes).expect("parse");
        assert_eq!(file.version, 2);
        assert_eq!(file.base.len(), 1);
        assert_eq!(file.custom.len(), 1);
        assert_eq!(file.custom[0].target_id().to_string(), "h001");
        assert_eq!(file.custom[0].changes[1].value, ChangeValue::Int(300));
    }

    #[test]
    fn object_file_extended_kinds_read_level_and_pointer() {
        let custom = vec![(
            FourCc::parse("AHbz").unwrap(),
            FourCc::parse("A001").unwrap(),
            vec![entry(b"Hbz1", ChangeValue::Int(7), 3, 1)],
        )];
        let bytes = build_object_file(ObjectKind::Ability, 2, &[], &custom);
        let file = ObjectDataFile::parse(ObjectKind::Ability, &bytes).expect("parse");
        let change = &file.custom[0].changes[0];
        assert_eq!(change.level, 3);
        assert_eq!(change.data_pointer, 1);
    }

    #[test]
    fn object_file_v3_skips_set_list() {
        let base = vec![(
            FourCc::parse("hfoo").unwrap(),
            FourCc::new([0; 4]),
            vec![entry(b"uhpm", ChangeValue::Int(1), 0, 0)],
        )];
        let bytes = build_object_file(ObjectKind::Unit, 3, &base, &[]);
        let file = ObjectDataFile::parse(ObjectKind::Unit, &bytes).expect("parse");
        assert_eq!(file.version, 3);
        assert_eq!(file.base[0].changes[0].value, ChangeValue::Int(1));
    }

    #[test]
    fn every_object_kind_parses_with_its_own_layout() {
        // `extended` kinds (ability, upgrade, doodad) carry level/variation +
        // data pointer per change; the rest are simple.
        for kind in ObjectKind::ALL {
            let custom = vec![(
                FourCc::parse("hfoo").unwrap(),
                FourCc::parse("h001").unwrap(),
                vec![
                    entry(b"unam", ChangeValue::String("custom".into()), 2, 1),
                    entry(b"uhpm", ChangeValue::Int(500), 0, 0),
                ],
            )];
            let bytes = build_object_file(kind, 2, &[], &custom);
            let file = ObjectDataFile::parse(kind, &bytes)
                .unwrap_or_else(|error| panic!("{:?}: {error}", kind));
            assert_eq!(file.kind, kind);
            assert_eq!(file.custom.len(), 1);
            let change = &file.custom[0].changes[0];
            if kind.extended() {
                assert_eq!(change.level, 2, "{:?} level", kind);
                assert_eq!(change.data_pointer, 1, "{:?} pointer", kind);
            } else {
                assert_eq!(change.level, 0, "{:?} simple change", kind);
            }
            assert_eq!(file.custom[0].changes[1].value, ChangeValue::Int(500));
        }
    }

    #[test]
    fn object_file_rejects_unknown_version() {
        let mut bytes = 9u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            ObjectDataFile::parse(ObjectKind::Unit, &bytes),
            Err(GamedataError::UnsupportedVersion { version: 9, .. })
        ));
    }

    #[test]
    fn wts_parses_multiline_blocks() {
        let text = "\u{feff}STRING 0\n{\nline one\nline two\n}\n\nSTRING 87\n{\nПривет\n}\n";
        let wts = Wts::parse(text.as_bytes()).expect("parse");
        assert_eq!(wts.len(), 2);
        assert_eq!(wts.get(0), Some("line one\nline two"));
        assert_eq!(wts.get(87), Some("Привет"));
        assert_eq!(wts.resolve("TRIGSTR_087"), "Привет");
        assert_eq!(wts.resolve("plain"), "plain");
    }

    #[test]
    fn wts_rejects_unterminated_block() {
        let text = "STRING 0\n{\nno end\n";
        assert!(matches!(Wts::parse(text.as_bytes()), Err(GamedataError::Wts(_))));
    }

    #[test]
    fn text_table_keeps_duplicates_and_components() {
        let text = "[Hamg]\nName=Archmage\nButtonpos=0,2\nArt=a.blp\n";
        let table = TextTable::parse("UnitStrings", text.as_bytes()).expect("parse");
        assert_eq!(table.get("Hamg", "name"), Some("Archmage"));
        assert_eq!(table.component("Hamg", "Buttonpos", 1), Some("2"));
        assert_eq!(table.section_count(), 2); // global + Hamg
    }

    #[test]
    fn metadata_column_resolution_matches_reference_rule() {
        let meta = b"ID;PWXL;N;E\nB;Y3;X6;D0\n\
C;Y1;X1;K\"ID\"\nC;X2;K\"field\"\nC;X3;K\"slk\"\nC;X4;K\"index\"\nC;X5;K\"repeat\"\nC;X6;K\"type\"\n\
C;Y2;X1;K\"Hbz1\"\nC;X2;K\"Data\"\nC;X3;K\"AbilityData\"\nC;X4;K\"-1\"\nC;X5;K\"4\"\nC;X6;K\"int\"\n\
C;Y3;X1;K\"unam\"\nC;X2;K\"Name\"\nC;X3;K\"Profile\"\nC;X4;K\"0\"\nC;X5;K\"0\"\nC;X6;K\"string\"\n";
        let table = MetaTable::parse(MetaKind::Ability, meta).expect("parse");
        let field = table.get("Hbz1").expect("field");
        assert_eq!(field.column_for(3, 1), "dataa3");
        assert_eq!(field.column_for(0, 0), "data");
        let profile = table.get("unam").expect("field");
        assert!(profile.is_profile());
        assert!(matches!(
            resolve_field(Some(&table), "unam", 0, 0),
            FieldTarget::Profile { .. }
        ));
        assert_eq!(resolve_field(Some(&table), "zzzz", 0, 0), FieldTarget::Unknown);
    }

    #[test]
    fn merge_applies_columns_profile_and_custom_rows() {
        let mut sheet = Sheet::parse("UnitData", SYNTHETIC).expect("parse");
        let meta_bytes = b"ID;PWXL;N;E\nB;Y3;X4;D0\n\
C;Y1;X1;K\"ID\"\nC;X2;K\"field\"\nC;X3;K\"slk\"\nC;X4;K\"index\"\n\
C;Y2;X1;K\"uhpm\"\nC;X2;K\"HP\"\nC;X3;K\"UnitData\"\nC;X4;K\"-1\"\n\
C;Y3;X1;K\"unam\"\nC;X2;K\"Name\"\nC;X3;K\"Profile\"\nC;X4;K\"0\"\n";
        let meta = MetaTable::parse(MetaKind::Unit, meta_bytes).expect("meta");
        let file = ObjectDataFile {
            kind: ObjectKind::Unit,
            version: 2,
            base: vec![ObjectEntry {
                old_id: FourCc::parse("hfoo").unwrap(),
                new_id: FourCc::new([0; 4]),
                changes: vec![entry(b"uhpm", ChangeValue::Int(999), 0, 0)],
            }],
            custom: vec![ObjectEntry {
                old_id: FourCc::parse("hfoo").unwrap(),
                new_id: FourCc::parse("h001").unwrap(),
                changes: vec![entry(b"unam", ChangeValue::String("Custom".into()), 0, 0)],
            }],
        };
        let mut profile = ProfileOverlay::default();
        let report = merge_overlay(
            ObjectKind::Unit,
            &file,
            Some(&meta),
            &mut [MergeTarget {
                name: "UnitData",
                sheet: &mut sheet,
                create_rows: true,
            }],
            &mut profile,
        );
        assert_eq!(report.applied, 1);
        assert_eq!(report.profile, 1);
        assert_eq!(report.created.len(), 1);
        let (value, origin) = sheet.cell_with_overlay("hfoo", "hp").expect("overlaid cell");
        assert_eq!(value.raw(), "999");
        assert_eq!(origin, Origin::Overlay);
        // The shipped value stays untouched underneath.
        assert_eq!(sheet.cell("hfoo", "hp").unwrap().raw(), "420");
        assert_eq!(sheet.cell("h001", "hp").unwrap().raw(), "420");
        assert_eq!(profile.get("h001", "Name", 0), Some("Custom"));
    }

    #[test]
    fn object_kind_extensions_match_hivewe_loader() {
        assert_eq!(ObjectKind::Unit.extension(), "w3u");
        assert_eq!(ObjectKind::Item.extension(), "w3t");
        assert_eq!(ObjectKind::Ability.extension(), "w3a");
        assert_eq!(ObjectKind::Upgrade.extension(), "w3q");
        assert_eq!(ObjectKind::Doodad.extension(), "w3d");
        assert_eq!(ObjectKind::Destructable.extension(), "w3b");
        assert_eq!(ObjectKind::Buff.extension(), "w3h");
        for kind in ObjectKind::ALL {
            assert_eq!(ObjectKind::from_extension(kind.extension()), Some(kind));
        }
        assert!(ObjectKind::Doodad.extended());
        assert!(ObjectKind::Ability.extended());
        assert!(!ObjectKind::Destructable.extended());
    }
}
