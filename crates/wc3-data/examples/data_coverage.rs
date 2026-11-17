//! Print the content-model coverage matrix as Markdown.
//!
//! ```text
//! cargo run -p wc3-data --example data_coverage [War3.mpq]
//! ```
//!
//! `analysis/agent-data/gen_data_coverage.sh` wraps this with the archive
//! provenance so `analysis/agent-data/DATA_COVERAGE.md` is reproducible.

use std::path::Path;

use wc3_data::source::{DirSource, Source};
use wc3_data::{DataError, GameDatabase};
use wc3_mpq::MpqArchive;

struct MpqSource {
    archive: MpqArchive,
}

impl Source for MpqSource {
    fn read(&self, path: &str) -> wc3_data::Result<Vec<u8>> {
        let normalized = path.replace('/', "\\");
        self.archive.read(&normalized).map_err(|source| DataError::Io {
            path: path.to_string(),
            source: std::io::Error::other(source.to_string()),
        })
    }

    fn describe(&self) -> String {
        "mpq".to_string()
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".to_string()
    });
    let path_ref = Path::new(&path);
    let source: Box<dyn Source> = if path_ref.is_dir() {
        Box::new(DirSource::new(path_ref))
    } else {
        Box::new(MpqSource {
            archive: MpqArchive::open(path_ref).expect("open mpq"),
        })
    };
    let data = GameDatabase::load(source.as_ref()).expect("load database");
    let report = data.coverage();

    println!("Archive: `{path}`\n");
    println!("Profile layers loaded: **{}**", data.localizer().loaded_files().len());
    let missing = data.localizer().missing_files();
    if !missing.is_empty() {
        println!(
            "\nProfile files the archive does not ship ({}): {}",
            missing.len(),
            missing
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!();
    print!("{}", report.markdown());
}
