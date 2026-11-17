//! Map-placed unit wiring: `war3mapUnits.doo` -> canonical Sim spawns.

use glam::Vec2;
use wc3_sim::{Sim, def_of};

fn human04_units() -> Option<Vec<wc3_formats::map::PlacedUnit>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
    if !path.is_file() {
        return None;
    }
    let base = wc3_formats::map::map_mpq_base(&path).ok()?;
    let archive = wc3_mpq::MpqArchive::open_at(&path, base).ok()?;
    let bytes = archive.read("war3mapUnits.doo").ok()?;
    wc3_formats::map::parse_units(&bytes).ok()
}

#[test]
fn human04_map_units_spawn_count_matches_parser() {
    let Some(placed) = human04_units() else {
        eprintln!("skip: Human04 map unit fixture missing/unparsable");
        return;
    };
    assert!(!placed.is_empty());
    // `spawn_map_units` accepts roster defs; report and filter the rest.
    let known: Vec<_> = placed
        .iter()
        .filter(|unit| def_of(&String::from_utf8_lossy(&unit.type_id)).is_some())
        .collect();
    let entries: Vec<(u8, String, Vec2)> = known
        .iter()
        .map(|unit| {
            (
                unit.player.min(1) as u8,
                String::from_utf8_lossy(&unit.type_id).to_string(),
                Vec2::new(unit.position.x, unit.position.y),
            )
        })
        .collect();
    let mut sim = Sim::new(7, 128, 128);
    let before = sim.alive_units().count();
    let refs: Vec<(u8, &str, Vec2)> = entries
        .iter()
        .map(|(owner, id, pos)| (*owner, id.as_str(), *pos))
        .collect();
    let spawned = sim.spawn_map_units(&refs);
    assert_eq!(
        spawned.len(),
        known.len(),
        "spawn count == parsed known count"
    );
    assert_eq!(sim.alive_units().count(), before + known.len());
    assert!(!spawned.is_empty(), "Human04 has at least one roster unit");
}
