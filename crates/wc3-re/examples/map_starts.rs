use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let path = std::env::args().nth(1).expect("map path");
    let bundle = wc3_re::mapfiles::MapBundle::open(Path::new(&path)).expect("open map");
    if let Some(info) = &bundle.info {
        println!("name={} players={}", info.name, info.recommended_players);
        let tail = &info.raw_tail;
        println!("tail_len={}", tail.len());
        let mut found = 0usize;
        let mut offset = 0usize;
        while offset + 64 <= tail.len() {
            let mut pts = Vec::new();
            for i in 0..8 {
                let base = offset + i * 8;
                let x = f32::from_le_bytes(tail[base..base + 4].try_into().unwrap());
                let y = f32::from_le_bytes(tail[base + 4..base + 8].try_into().unwrap());
                if !(0.0..=40000.0).contains(&x) || !(0.0..=40000.0).contains(&y) {
                    break;
                }
                pts.push((x.round(), y.round()));
            }
            if pts.len() == 8 && pts.iter().filter(|p| **p != (0.0, 0.0)).count() >= 2 {
                println!("start_locations@{offset}={pts:?}");
                found += 1;
                if found >= 3 { break; }
            }
            offset += 4;
        }
    }
    if let Some(terrain) = &bundle.terrain {
        let size = (terrain.width as f32 * 128.0, terrain.height as f32 * 128.0);
        println!(
            "terrain {}x{} center=({:.0},{:.0}) size=({:.0},{:.0}) origin=({:.0},{:.0})",
            terrain.width,
            terrain.height,
            terrain.center.0,
            terrain.center.1,
            size.0,
            size.1,
            terrain.center.0 - size.0 * 0.5,
            terrain.center.1 - size.1 * 0.5
        );
    }
    for (file, error) in &bundle.errors {
        println!("map_error {file}: {error}");
    }
    println!("doodads_present={}", bundle.doodads.is_some());
    if let Some(doodads) = &bundle.doodads {
        let mut ids: BTreeMap<String, usize> = BTreeMap::new();
        for doodad in doodads {
            let id = String::from_utf8_lossy(&doodad.type_id).into_owned();
            *ids.entry(id).or_default() += 1;
        }
        println!("doo_doodads={} unique={}", doodads.len(), ids.len());
        for (id, count) in ids.iter().take(20) {
            println!("  doodad {id} x{count}");
        }
    }
    let units = bundle.units.as_deref().unwrap_or(&[]);
    let mut by_id: BTreeMap<String, usize> = BTreeMap::new();
    for unit in units {
        let id = String::from_utf8_lossy(&unit.type_id).into_owned();
        *by_id.entry(id).or_default() += 1;
    }
    println!("doo_units={} unique={}", units.len(), by_id.len());
    for (id, count) in by_id.iter().take(20) {
        println!("  {id} x{count}");
    }
    let mines = units
        .iter()
        .filter(|unit| {
            let id = String::from_utf8_lossy(&unit.type_id).to_ascii_lowercase();
            id == "ngol" || id == "ugol" || id == "agol"
        })
        .count();
    println!("gold_mine_units={mines}");
    for unit in units.iter().take(10) {
        let id = String::from_utf8_lossy(&unit.type_id);
        println!("  unit {id} player={} pos=({:.0},{:.0},{:.0})", unit.player, unit.position.x, unit.position.y, unit.position.z);
    }
}
