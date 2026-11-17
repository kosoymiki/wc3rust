//! Inventory of movie files in a directory: which of them the decoder can open,
//! their container, codec fourcc, size and frame count.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example extract_movies -- <dir> [<dir>...]
//! ```
//!
//! Note on the archives: the retail `War3.mpq` / `War3x.mpq` data archives
//! contain **no** `Movies\*.bik` entries (verified: listfile, hash-probed
//! engine paths and an unnamed-block BINK magic scan all return zero), so
//! there is nothing to extract from them. The cinematics live on the installer
//! discs as AVI files named `*.mpq` / `*.blz`; point this tool at that folder.

use std::path::Path;

use wc3_bink::BinkFile;

fn visit(dir: &Path, rows: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            visit(&path, rows);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(ext.to_ascii_lowercase().as_str(), "bik" | "mpq" | "avi" | "blz") {
            continue;
        }
        match BinkFile::open(&path) {
            Ok(file) => {
                let v = file.video();
                let audio = file
                    .audio_info()
                    .map(|a| format!("mp3 {}Hz {}ch", a.sample_rate, a.channels))
                    .unwrap_or_else(|| "no audio".into());
                rows.push(format!(
                    "{:<18} {:>10} B  AVI {}x{}  {} frames  {:.2} fps  {:.1} s  {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    path.metadata().map(|m| m.len()).unwrap_or(0),
                    v.width,
                    v.height,
                    file.frame_count(),
                    v.fps(),
                    file.duration_secs(),
                    audio
                ));
            }
            Err(err) => rows.push(format!(
                "{:<18} not decodable: {err}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
}

fn main() {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        eprintln!("usage: extract_movies <dir> [<dir>...]");
        std::process::exit(2);
    }
    let mut rows = Vec::new();
    for dir in &dirs {
        visit(Path::new(dir), &mut rows);
    }
    for row in &rows {
        println!("{row}");
    }
    println!("{} candidate file(s)", rows.len());
}
