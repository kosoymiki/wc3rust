//! Canonical SLK value dump: `id|column=value|column=value…` per data row.
//!
//! Reference side of `scripts/slk_value_compare.py`, which re-parses the same
//! shipped table with an independent reader. Only non-empty cells are printed,
//! columns inside a row are sorted by name, rows keep the sheet order.
//!
//! Usage: cargo run -q -p wc3-formats --example slk_dump -- <file.slk> [...]

use std::path::Path;

use wc3_formats::slk::SlkTable;

fn main() {
    let mut failed = false;
    for arg in std::env::args().skip(1) {
        let path = Path::new(&arg);
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("{}: read error {error}", path.display());
                failed = true;
                continue;
            }
        };
        match SlkTable::parse(&bytes) {
            Ok(table) => {
                for row in &table.rows {
                    let id = row.first().cloned().unwrap_or_default();
                    let mut fields: Vec<(String, String)> = Vec::new();
                    for (index, column) in table.columns.iter().enumerate().skip(1) {
                        if let Some(value) = row.get(index) {
                            if !value.is_empty() {
                                fields.push((column.clone(), value.clone()));
                            }
                        }
                    }
                    fields.sort_by(|a, b| a.0.cmp(&b.0));
                    let mut line = id;
                    for (column, value) in fields {
                        line.push('|');
                        line.push_str(&column);
                        line.push('=');
                        line.push_str(&value);
                    }
                    println!("{line}");
                }
            }
            Err(error) => {
                eprintln!("{}: parse error {error:?}", path.display());
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
