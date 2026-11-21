//! SLK spreadsheet parser (Warcraft III `*.slk` data tables).

use std::collections::BTreeMap;

use crate::FormatError;

/// Ceiling on the dense grid this reader materialises. The widest/longest
/// shipped sheet is far below it; a declared coordinate above it is corruption
/// and must not turn into a multi-gigabyte allocation.
const MAX_ROWS: usize = 65_536;
const MAX_COLUMNS: usize = 1_024;

/// Ceiling on the total cells the grid may materialise.
///
/// `MAX_ROWS * MAX_COLUMNS` is not a useful bound by itself: rows and columns
/// were checked independently, so one forged `C;X1024;Y65536` in a 31-byte file
/// expanded to a dense 65 536 × 1 024 grid (WC3-SLK-001). The largest shipped
/// sheet measured on this install is `Units/AbilityData.slk`
/// (56 × 400 = 22 400 cells), so one million cells leaves a 46× margin.
const MAX_CELLS: usize = 1_048_576;

#[derive(Debug, Clone, Default)]
pub struct SlkTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    cells: BTreeMap<(u32, u32), String>,
    max_x: u32,
    max_y: u32,
}

impl SlkTable {
    /// Parse an SLK file (`ID;PWXL`). Quoted strings use `""` for a quote.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let text =
            std::str::from_utf8(bytes).map_err(|_| FormatError::Corrupt("SLK is not UTF-8"))?;
        let mut table = SlkTable::default();
        let mut current_x = 0u32;
        let mut current_y = 0u32;
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.len() < 2 {
                continue;
            }
            let mut fields = line.split(';');
            let kind = fields.next().unwrap_or("");
            match kind {
                "C" | "F" => {
                    let mut x = current_x;
                    let mut y = current_y;
                    let mut value = String::new();
                    let mut has_value = false;
                    for field in fields {
                        if let Some(rest) = field.strip_prefix('X') {
                            x = rest.parse().unwrap_or(0);
                        } else if let Some(rest) = field.strip_prefix('Y') {
                            y = rest.parse().unwrap_or(0);
                        } else if let Some(rest) = field.strip_prefix('K') {
                            value = unquote(rest);
                            has_value = true;
                        } else if field == "E" || field.is_empty() {
                            // empty cell marker
                        }
                    }
                    if has_value && x > 0 && y > 0 {
                        if x as usize > MAX_COLUMNS || y as usize > MAX_ROWS {
                            // A forged coordinate used to size the dense grid;
                            // skip the cell (the strict reader reports it too).
                            crate::diag::note("slk_position_out_of_range", || {
                                format!("x={x} y={y} limit={MAX_ROWS}x{MAX_COLUMNS}")
                            });
                        } else {
                            table.cells.insert((x - 1, y - 1), value);
                            table.max_x = table.max_x.max(x);
                            table.max_y = table.max_y.max(y);
                        }
                    }
                    current_x = x;
                    current_y = y;
                }
                "B" => {
                    // `B;X<cols>;Y<rows>` declares the sheet extent. It is never
                    // trusted for sizing (it is only advisory), but a forged
                    // declaration is reported instead of silently ignored.
                    let mut bx = 0usize;
                    let mut by = 0usize;
                    for field in fields {
                        if let Some(rest) = field.strip_prefix('X') {
                            bx = rest.parse().unwrap_or(0);
                        } else if let Some(rest) = field.strip_prefix('Y') {
                            by = rest.parse().unwrap_or(0);
                        }
                    }
                    if bx.saturating_mul(by) > MAX_CELLS {
                        crate::diag::note("slk_declared_grid_too_large", || {
                            format!("declared={bx}x{by} cell_limit={MAX_CELLS}")
                        });
                    }
                }
                _ => {}
            }
        }
        let width = table.max_x as usize;
        let height = table.max_y as usize;
        if width == 0 || height == 0 {
            return Ok(table);
        }
        if width.saturating_mul(height) > MAX_CELLS {
            return Err(FormatError::Corrupt("SLK grid exceeds the cell budget"));
        }
        for x in 0..width {
            table
                .columns
                .push(table.cells.get(&(x as u32, 0)).cloned().unwrap_or_default());
        }
        for y in 1..height {
            let mut row = Vec::with_capacity(width);
            for x in 0..width {
                row.push(
                    table
                        .cells
                        .get(&(x as u32, y as u32))
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            table.rows.push(row);
        }
        Ok(table)
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(name))
    }

    pub fn cell(&self, row: usize, column: &str) -> Option<&str> {
        let index = self.column_index(column)?;
        self.rows.get(row)?.get(index).map(String::as_str)
    }

    pub fn row_map(&self, row: usize) -> BTreeMap<&str, &str> {
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

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn unquote(value: &str) -> String {
    let value = value.strip_prefix('"').unwrap_or(value);
    let value = value.strip_suffix('"').unwrap_or(value);
    value.replace("\"\"", "\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_synthetic_sheet() {
        let slk = b"ID;PWXL;N;E\nB;X2;Y2;D0\nC;X1;Y1;K\"unitID\"\nC;X2;Y1;K\"HP\"\n\
                    C;X1;Y2;K\"hfoo\"\nC;X2;Y2;K420\nC;X1;Y3;K\"hpea\"\nC;X2;Y3;K220\nE\n";
        let table = SlkTable::parse(slk).unwrap();
        assert_eq!(table.columns, vec!["unitID", "HP"]);
        assert_eq!(table.len(), 2);
        assert_eq!(table.cell(0, "unitID"), Some("hfoo"));
        assert_eq!(table.cell(1, "hp"), Some("220"));
    }

    #[test]
    fn parses_real_unit_data() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_samples/Units/UnitData.slk");
        let Ok(bytes) = fs::read(&path) else {
            eprintln!("skipping: UnitData.slk fixture missing");
            return;
        };
        let table = SlkTable::parse(&bytes).unwrap();
        assert!(table.columns.len() > 20, "columns: {}", table.columns.len());
        assert!(table.len() > 100, "rows: {}", table.len());
        assert!(table.column_index("unitID").is_some());
    }
}
