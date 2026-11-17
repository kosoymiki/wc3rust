//! Which corpus maps carry `war3map.wts` + object data that overrides a name
//! with a `TRIGSTR_` reference.
//!
//! This is how the `DotF_v1.4.w3x` fixture in `tests/data_coverage.rs` was
//! chosen: it has 137 wts strings, a custom unit `U000` (base `Ucrl`) whose
//! `unam` is `TRIGSTR_013`, and matching overrides for an ability and an item.
//!
//! ```text
//! cargo run -p wc3-data --example wts_probe
//! ```

use std::path::PathBuf;

use wc3_formats::gamedata::{ObjectDataFile, ObjectKind, Wts};
use wc3_formats::map::map_mpq_base;
use wc3_mpq::MpqArchive;

fn main() {
    let root = PathBuf::from(
        "/data/data/com.termux/files/home/src/herofolends-rs/donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps",
    );
    let mut maps = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("w3m") | Some("w3x")
            ) {
                maps.push(path);
            }
        }
    }
    maps.sort();
    for map in &maps {
        let Ok(base) = map_mpq_base(map) else { continue };
        let Ok(archive) = MpqArchive::open_at(map, base) else { continue };
        let Ok(wts_bytes) = archive.read("war3map.wts") else { continue };
        let Ok(wts) = Wts::parse(&wts_bytes) else { continue };
        let wts_count = wts.iter().count();
        for (name, kind) in [
            ("war3map.w3u", ObjectKind::Unit),
            ("war3map.w3t", ObjectKind::Item),
            ("war3map.w3a", ObjectKind::Ability),
            ("war3map.w3q", ObjectKind::Upgrade),
        ] {
            let Ok(bytes) = archive.read(name) else { continue };
            let Ok(file) = ObjectDataFile::parse(kind, &bytes) else { continue };
            let mut trig = Vec::new();
            for entry in file.base.iter().chain(file.custom.iter()) {
                for change in &entry.changes {
                    let value = change.value.as_text();
                    if value.starts_with("TRIGSTR_") {
                        trig.push(format!(
                            "{} {} {} {}",
                            entry.old_id, entry.new_id, change.id, value
                        ));
                    }
                }
            }
            if !trig.is_empty() {
                println!(
                    "{} :: {name} :: wts_entries={wts_count} entries={} trigstr={}",
                    map.display(),
                    file.base.len() + file.custom.len(),
                    trig.len()
                );
                for line in trig.iter().take(6) {
                    println!("    {line}");
                }
            }
        }
    }
}
