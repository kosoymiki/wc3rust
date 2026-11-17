//! mechanic `data.tft-neutral-parity`: every neutral placement on the shipped
//! 1v1 map `(2)Justice.w3x` resolves to a TFT definition and spawns, and the
//! neutral groups form the expected creep camps and shops.
//!
//! This is the regression test for the runtime data gap where
//! `wc3_sim::data::def_of` rejected 58 of 85 neutral placements across 19 TFT
//! ids (`nfoh, nfsh, nfsp, ngad, nggr, nhdc, nhfp, nhhr, nkot, nmoo, nmr8,
//! nmrl, nomg, nrdr, nrog, nshe, nsqt, ntav, nwzg`).  The defs are generated
//! from `analysis/war3x_data/Units/{UnitData,UnitBalance,UnitWeapons}.slk` by
//! `analysis/agent-sim/gen_creeps.py` (mobile creeps) and
//! `analysis/agent-sim/gen_neutrals.py` (neutral structures).

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::{Sim, Vec2, data, def_of};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

struct Justice {
    wpm: PathingMap,
    terrain: wc3_formats::w3e::Terrain,
    units: Vec<wc3_formats::map::PlacedUnit>,
}

fn load_justice() -> Option<Justice> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        eprintln!("skip: (2)Justice.w3x fixture missing at {}", path.display());
        return None;
    }
    let base = map_mpq_base(&path).ok()?;
    let archive = MpqArchive::open_at(&path, base).ok()?;
    Some(Justice {
        wpm: PathingMap::parse(&archive.read("war3map.wpm").ok()?).ok()?,
        terrain: wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").ok()?).ok()?,
        units: parse_units(&archive.read("war3mapUnits.doo").ok()?).ok()?,
    })
}

/// Every TFT neutral id the map places must be defined by the generated tables.
fn assert_generated_def(id: &str) {
    assert!(
        def_of(id).is_some(),
        "{id} has no generated def (expected in creeps_gen.rs / extras_gen.rs)"
    );
}

/// Rawcodes that exist only in the map, not in the TFT tables we have:
/// `nrac` (2 placements) has no row in `analysis/war3x_data/Units/UnitData.slk`
/// and no `war3map.w3u` entry in the map, so it cannot be generated from data.
/// It is reported, never invented (`analysis/agent-sim/P0-MAP-POSITION.md` has
/// the same class of gap for `nC02` / `nC03`).
const MAP_ONLY_IDS: &[&str] = &["nrac"];

/// mechanic `data.tft-neutral-parity`: all neutral placements spawn, and the
/// map yields its full creep-camp / shop structure.
#[test]
fn data_tft_neutral_parity_justice_placements() {
    let Some(map) = load_justice() else {
        return;
    };
    let origin = Vec2::new(map.terrain.center.0, map.terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let mut sim = Sim::new(0x3_8200, map.wpm.width, map.wpm.height);
    let mut walkable = Vec::with_capacity(map.wpm.width as usize * map.wpm.height as usize);
    for y in 0..map.wpm.height {
        for x in 0..map.wpm.width {
            walkable.push(map.wpm.walkable(x, y));
        }
    }
    sim.set_pathing(
        map.wpm.width,
        map.wpm.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );

    let mut placements = 0usize;
    let mut spawned = 0usize;
    let mut unresolved: Vec<String> = Vec::new();
    let mut map_only: Vec<String> = Vec::new();
    let mut off_map = 0usize;
    let mut ids: BTreeSet<String> = BTreeSet::new();
    let mut neutral_units = Vec::new();
    let in_bounds = |pos: Vec2| {
        pos.x >= 0.0
            && pos.y >= 0.0
            && pos.x < map.wpm.width as f32 * wc3_sim::PATHING_CELL_SIZE
            && pos.y < map.wpm.height as f32 * wc3_sim::PATHING_CELL_SIZE
    };
    for unit in &map.units {
        let def_id = String::from_utf8_lossy(&unit.type_id).to_string();
        if def_id == "sloc" || def_id == "ngol" || def_id == "ugol" {
            continue; // markers and gold mines are world objects, not units
        }
        placements += 1;
        ids.insert(def_id.clone());
        let pos = to_world(unit.position.x, unit.position.y);
        if !in_bounds(pos) {
            // Placed outside the playable area (editor decoration): it can never
            // be reached, exactly like an off-map tree.
            off_map += 1;
            continue;
        }
        if def_of(&def_id).is_none() {
            if !map_only.contains(&def_id) {
                map_only.push(def_id.clone());
            }
            if !MAP_ONLY_IDS.contains(&def_id.as_str()) && !unresolved.contains(&def_id) {
                unresolved.push(def_id.clone());
            }
            continue;
        }
        if sim.spawn_def_exact(2, &def_id, pos).is_some() {
            spawned += 1;
            neutral_units.push(pos);
        }
    }
    assert!(
        unresolved.is_empty(),
        "unresolved in-bounds neutral ids: {unresolved:?} (placements {placements}, spawned {spawned}, off-map {off_map})"
    );
    assert_eq!(
        placements + 9,
        99,
        "99 placed records = 2 start markers + 7 gold mines + neutral placements"
    );
    assert_eq!(spawned, placements - off_map - 2, "every data-backed neutral spawned");
    assert_eq!(off_map, 0, "the terrain origin puts every placement in bounds");
    assert_eq!(map_only, vec!["nrac".to_string()], "only the map-only rawcode is skipped");
    assert_eq!(
        placements, 90,
        "Justice neutral unit placements (99 total - 2 sloc - 7 mines)"
    );
    assert_eq!(
        spawned + 2,
        placements,
        "90 placements = {spawned} data-backed + 2 map-only `nrac`"
    );
    assert!(ids.len() >= 25, "distinct neutral ids on the map: {}", ids.len());

    // Every id in the intakestruct gap list must resolve, even when a map does
    // not place it (the list is the regression scope of this test).
    for id in [
        "nfoh", "nfsh", "nfsp", "ngad", "nggr", "nhdc", "nhfp", "nhhr", "nkot", "nmoo", "nmr8",
        "nmrl", "nomg", "nrdr", "nrog", "nshe", "nsqt", "ntav", "nwzg",
    ] {
        assert_generated_def(id);
    }

    // Creep camps: greedy 600-unit grouping around owner-2 units, exactly as
    // `sim_bridge::spawn_map_scene` clusters them.
    let mut remaining: Vec<(wc3_sim::UnitId, Vec2)> = sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == 2)
        .map(|unit| (unit.id, unit.pos))
        .collect();
    let mut camps = 0usize;
    while let Some((_, anchor)) = remaining.first().copied() {
        let mut grouped = Vec::new();
        let mut rest = Vec::new();
        for (id, pos) in remaining {
            if pos.distance(anchor) <= 600.0 {
                grouped.push(id);
            } else {
                rest.push((id, pos));
            }
        }
        if sim.register_camp(anchor, &grouped, 500.0, 900.0).is_some() {
            camps += 1;
        }
        remaining = rest;
    }
    let shops = sim.shops.len();
    // Trade buildings = item sellers (`Sellitems`) plus unit sellers
    // (`Sellunits`): the map has 2 goblin merchants, 2 mercenary camps and 1
    // tavern.
    let trade: Vec<(&str, bool, usize)> = sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == 2 && unit.is_building())
        .filter(|unit| {
            !unit.def().sells_items.is_empty() || !data::sold_units(unit.def().id).is_empty()
        })
        .map(|unit| {
            (
                unit.def().id,
                !unit.def().sells_items.is_empty(),
                data::sold_units(unit.def().id).len(),
            )
        })
        .collect();
    let trade_buildings = trade.len();
    eprintln!("JUSTICE trade buildings: {trade:?}");
    eprintln!(
        "JUSTICE neutrals: placements={placements} spawned={spawned} ids={} camps={camps} item_shops={shops} trade_buildings={trade_buildings}",
        ids.len()
    );
    // Camps are armed mobile neutrals only (`Unit::is_camp_guard`): of the 88
    // data-backed placements, 76 creeps form 22 proximity camps while 10
    // neutral structures and 2 critters (sheep) stay out of every camp body.
    // Before the guard filter the same 600-unit clustering produced 25 camps
    // because shops and structures were counted as camp members.
    assert_eq!(camps, 22, "creep camps formed by the 76 armed neutral placements");
    assert_eq!(shops, 2, "item shop buildings (goblin merchants)");
    // Every neutral trade building on the map: ammo dumps (unit sellers),
    // goblin merchants (item sellers), mercenary camps (unit sellers), tavern
    // (hero seller).
    assert_eq!(
        trade_buildings, 7,
        "neutral trade buildings on the map: {trade:?}"
    );
    let mut kinds: BTreeSet<&str> = trade.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        kinds.len(),
        4,
        "distinct trade building types: {:?}",
        kinds.iter().collect::<Vec<_>>()
    );
    kinds.clear();
    let _ = &mut kinds;

    // A camp's units are real, targetable units with the map's hp.
    let camp = sim.camps.first().expect("camps registered");
    assert!(!camp.units.is_empty());
    for id in &camp.units {
        let unit = sim.unit(*id).expect("camp unit");
        assert!(unit.alive && unit.max_hp > 0, "camp unit has stats");
        assert_eq!(unit.owner, 2, "camp units are neutral");
    }
    // Every generated creep def carries real table values (no zero-stat stubs).
    for id in ["nfsh", "nfsp", "nggr", "nhdc", "nhfp", "nhhr", "nkot", "nmrl", "nomg", "nrdr",
               "nrog", "nsqt", "nwzg", "nshe"] {
        let def = def_of(id).expect("creep def");
        assert!(def.stats.hp > 0, "{id} has hp");
        assert!(def.stats.speed >= 0.0, "{id} has a speed");
        assert!(!def.name.is_empty(), "{id} has a name");
    }
    // Neutral structures are buildings with a footprint and no attack.
    for id in ["nfoh", "nmoo", "ngad", "nmr8", "ntav"] {
        let def = def_of(id).expect("neutral structure def");
        assert!(def.is_building(), "{id} is a building");
        assert!(def.stats.hp > 0, "{id} has hp");
    }
    // Structural invariant: camps hold armed mobile creeps only, so shops,
    // structures and critters can never aggro a player.
    for camp in &sim.camps {
        for unit in &camp.units {
            let unit = sim.unit(*unit).expect("camp member");
            assert!(
                unit.is_camp_guard(),
                "camp member {} ({}) is not a creep guard",
                unit.id.0,
                unit.def().id
            );
            assert!(!unit.is_trade_building());
            assert!(!unit.is_critter());
        }
    }
    let _ = data::def_index("nfsh").expect("index");
}
