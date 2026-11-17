//! Prints the placed-unit census of a `.w3x`/`.w3m` map.
//!
//!   cargo run -p wc3-sim --example map_census -- <map path>

use std::collections::BTreeMap;
use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_mpq::MpqArchive;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: map_census <map> [--objects]");
        return;
    };
    let objects = std::env::args().any(|arg| arg == "--objects");
    let path = Path::new(&path);
    let base = map_mpq_base(path).expect("mpq base");
    let archive = MpqArchive::open_at(path, base).expect("mpq");
    if objects {
        let bytes = archive.read("war3map.w3u").expect("war3map.w3u");
        let data = wc3_formats::objects::ObjectData::parse(&bytes).expect("parse w3u");
        println!("version {} entries {}", data.version, data.entries.len());
        for entry in &data.entries {
            println!(
                "old={} new={} mods={}",
                String::from_utf8_lossy(&entry.old_id),
                String::from_utf8_lossy(&entry.new_id),
                entry.modifications.len()
            );
        }
        return;
    }
    let units = parse_units(&archive.read("war3mapUnits.doo").expect("units")).expect("parse");
    let mut census: BTreeMap<String, (usize, u32)> = BTreeMap::new();
    let wanted = std::env::var("CENSUS_ID").ok();
    for unit in &units {
        let id = String::from_utf8_lossy(&unit.type_id).to_string();
        if wanted.as_deref() == Some(id.as_str()) {
            println!(
                "{id} player={} pos=({:.1},{:.1}) rot={:.2} scale=({:.2},{:.2},{:.2}) hp={} mp={} hero_level={} variation={}",
                unit.player,
                unit.position.x,
                unit.position.y,
                unit.rotation,
                unit.scale.x,
                unit.scale.y,
                unit.scale.z,
                unit.hp,
                unit.mp,
                unit.hero_level,
                unit.variation
            );
        }
        let entry = census.entry(id).or_insert((0, unit.player));
        entry.0 += 1;
    }
    let mut rows: Vec<_> = census.into_iter().collect();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
    for (id, (count, player)) in rows {
        println!("{id:>6}  x{count:<3} player={player}");
    }
    println!("total placed: {}", units.len());
}
