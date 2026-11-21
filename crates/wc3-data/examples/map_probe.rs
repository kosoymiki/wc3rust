//! Inspect a `.w3x`/`.w3m`: map name, placed doodads and their ids, and the
//! object-data files the map carries.
//!
//! This is the measurement behind `tests/data_tables.rs::economy_lumber_doodad_defs`
//! (the shipped melee map is the lumber oracle). It answers "which map is this
//! and what does it place" without loading the whole typed database.
//!
//! ```text
//! cargo run -p wc3-data --example map_probe -- analysis/war3_map/Maps/Melee.w3x
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_doodads_ex};
use wc3_formats::w3i::MapInfo;
use wc3_mpq::MpqArchive;

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "analysis/war3_map/Maps/Melee.w3x".to_string());
    let path = Path::new(&path);
    let base = map_mpq_base(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let archive = MpqArchive::open_at(path, base).map_err(|error| format!("open: {error}"))?;

    if let Ok(bytes) = archive.read("war3map.w3i") {
        let wts = archive
            .read("war3map.wts")
            .ok()
            .and_then(|bytes| wc3_formats::gamedata::Wts::parse(&bytes).ok());
        match MapInfo::parse(&bytes) {
            Ok(info) => {
                let resolve = |text: &str| match &wts {
                    Some(wts) => wts.resolve(text),
                    None => text.to_string(),
                };
                println!(
                    "MAP name={:?} author={:?} players={:?} version={} wts={}",
                    resolve(&info.name),
                    resolve(&info.author),
                    resolve(&info.recommended_players),
                    info.version,
                    wts.is_some()
                );
            }
            Err(error) => println!("MAP w3i unparsed: {error}"),
        }
    }

    let bytes = archive
        .read("war3map.doo")
        .map_err(|error| format!("war3map.doo: {error}"))?;
    let file = parse_doodads_ex(&bytes).map_err(|error| format!("parse war3map.doo: {error}"))?;
    let mut by_id: BTreeMap<String, usize> = BTreeMap::new();
    for doodad in &file.doodads {
        let id = String::from_utf8_lossy(&doodad.type_id)
            .trim_end_matches('\0')
            .to_string();
        *by_id.entry(id).or_default() += 1;
    }
    println!(
        "DOODADS placed={} distinct={} special={}",
        file.doodads.len(),
        by_id.len(),
        file.special.len()
    );
    for (id, count) in &by_id {
        println!("DOODAD {id} {count}");
    }

    let object_files = [
        "war3map.w3u",
        "war3map.w3t",
        "war3map.w3b",
        "war3map.w3d",
        "war3map.w3a",
        "war3map.w3q",
        "war3map.w3h",
        "war3map.w3s",
    ];
    let present: Vec<&str> = object_files
        .into_iter()
        .filter(|name| archive.read(name).is_ok())
        .collect();
    println!("OBJECT_DATA {present:?}");
    Ok(())
}
