//! Walk extracted WC3 data tables and report SLK row/column counts.
use std::path::Path;
use wc3_formats::slk::SlkTable;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("slk")) == Some(true) {
                out.push(path);
            }
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "analysis/war3_data".into());
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();
    let mut total_rows = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        match SlkTable::parse(&bytes) {
            Ok(table) => {
                total_rows += table.rows.len();
                println!(
                    "{} rows={} cols={} first_id={:?}",
                    path.strip_prefix(&root).unwrap_or(path).display(),
                    table.rows.len(),
                    table.columns.len(),
                    table.rows.first().map(|row| row.get(0).cloned().unwrap_or_default())
                );
            }
            Err(error) => println!("FAIL {} {}", path.display(), error),
        }
    }
    println!("TOTAL files={} rows={}", files.len(), total_rows);
}
