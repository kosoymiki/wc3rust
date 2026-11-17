//! Census of the neutral placements on `(2)Justice.w3x`: def-id histogram,
//! which neutrals are combat creeps vs structures/shops/critters, and how the
//! 600-unit greedy clustering groups them into camps.
//!
//!   cargo run -p wc3-sim --example creep_census --offline

use std::collections::BTreeMap;
use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::{Sim, Vec2, data, def_of};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    assert!(path.is_file(), "missing fixture {}", path.display());
    let base = map_mpq_base(&path).unwrap();
    let archive = MpqArchive::open_at(&path, base).unwrap();
    let wpm = PathingMap::parse(&archive.read("war3map.wpm").unwrap()).unwrap();
    let terrain = wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").unwrap()).unwrap();
    let units = parse_units(&archive.read("war3mapUnits.doo").unwrap()).unwrap();
    let origin = Vec2::new(terrain.center.0, terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);

    let mut sim = Sim::new(0x3_8200, wpm.width, wpm.height);
    let mut walkable = Vec::with_capacity((wpm.width * wpm.height) as usize);
    for y in 0..wpm.height {
        for x in 0..wpm.width {
            walkable.push(wpm.walkable(x, y));
        }
    }
    sim.set_pathing(wpm.width, wpm.height, wc3_sim::PATHING_CELL_SIZE, walkable);

    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut spawned: Vec<(wc3_sim::UnitId, Vec2, &'static str)> = Vec::new();
    for unit in &units {
        let def_id = String::from_utf8_lossy(&unit.type_id).to_string();
        if def_id == "sloc" || def_id == "ngol" || def_id == "ugol" {
            continue;
        }
        if def_of(&def_id).is_none() {
            *hist.entry(format!("{def_id} (MISSING)")).or_default() += 1;
            continue;
        }
        let pos = to_world(unit.position.x, unit.position.y);
        if let Some(id) = sim.spawn_def_exact(2, &def_id, pos) {
            *hist.entry(def_id.clone()).or_default() += 1;
            spawned.push((id, pos, def_of(&def_id).unwrap().id));
        }
    }
    println!("neutral def histogram ({} spawned):", spawned.len());
    for (id, count) in &hist {
        let def = def_of(id.split(' ').next().unwrap());
        let kind = def.map(|def| {
            format!(
                "build={} damage={} speed={:.0} sight={:.0} level={} sells={} ",
                def.stats.is_building,
                def.stats.damage,
                def.stats.speed,
                def.stats.sight,
                def.stats.level,
                def.sells_items.len()
            )
        });
        println!("  {count:3} x {id:6} {}", kind.unwrap_or_default());
    }

    // Combat creeps only: alive, not a building, deals damage.
    let combat: Vec<(wc3_sim::UnitId, Vec2)> = spawned
        .iter()
        .filter(|(id, _, _)| {
            let unit = sim.unit(*id).unwrap();
            !unit.is_building() && (unit.damage > 0 || unit.def().stats.damage > 0)
        })
        .map(|(id, pos, _)| (*id, *pos))
        .collect();
    println!("combat creeps: {}", combat.len());
    let name_of = |id: wc3_sim::UnitId| sim.unit(id).unwrap().def().id;
    if std::env::var("DUMP").is_ok() {
        for (id, pos) in &combat {
            println!("  {:<6} {:>8.1} {:>8.1}", name_of(*id), pos.x, pos.y);
        }
        // Single-linkage groups at 400 units.
        let mut parent: Vec<usize> = (0..combat.len()).collect();
        fn find(parent: &mut Vec<usize>, i: usize) -> usize {
            if parent[i] != i {
                let root = find(parent, parent[i]);
                parent[i] = root;
            }
            parent[i]
        }
        for i in 0..combat.len() {
            for j in i + 1..combat.len() {
                if combat[i].1.distance(combat[j].1) <= 400.0 {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                    if a != b {
                        parent[a] = b;
                    }
                }
            }
        }
        let mut groups: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        for i in 0..combat.len() {
            let root = find(&mut parent, i);
            groups.entry(root).or_default().push(name_of(combat[i].0));
        }
        println!("single-linkage 400 groups: {}", groups.len());
        for (_, members) in &groups {
            println!("  {} x {:?}", members.len(), members);
        }
    }

    let mobile: Vec<(wc3_sim::UnitId, Vec2)> = spawned
        .iter()
        .filter(|(id, _, _)| !sim.unit(*id).unwrap().is_building())
        .map(|(id, pos, _)| (*id, *pos))
        .collect();
    for radius in [400.0f32, 450.0, 500.0, 512.0, 550.0, 600.0, 650.0] {
        for (label, set) in [("combat", &combat), ("mobile", &mobile)] {
            let mut remaining = set.clone();
            let mut camps = 0usize;
            let mut sizes = Vec::new();
            while let Some((_, anchor)) = remaining.first().copied() {
                let mut grouped = Vec::new();
                let mut rest = Vec::new();
                for (id, pos) in remaining {
                    if pos.distance(anchor) <= radius {
                        grouped.push(id);
                    } else {
                        rest.push((id, pos));
                    }
                }
                sizes.push(grouped.len());
                camps += 1;
                remaining = rest;
            }
            println!("{label} radius {radius}: camps={camps} sizes={sizes:?}");
        }
    }
    let _ = data::def_of("nftr");
}
