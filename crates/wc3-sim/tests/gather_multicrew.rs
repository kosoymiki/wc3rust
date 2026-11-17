//! Multi-worker gather on the real (2)Justice map: the exact melee opening the
//! game builds (hall at the start location, five workers on a 300-unit ring,
//! `nearest_gold_mine`) plus one `Command::Harvest` for the whole crew.
//!
//! The game path (`Game::issue` -> `Game::update` -> `Sim::step`) issues one
//! command for five workers; the single-worker economy tests pass, so this
//! test pins the difference down and prints the per-worker phase machine every
//! 60 ticks (`MULTIDBG` lines) for diagnosis.

use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_doodads, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::ai::{hall_for_race, worker_for_race};
use wc3_sim::{Command, HarvestPhase, Order, Race, Sim, Vec2, data};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn justice_sim() -> Option<(Sim, Vec<wc3_sim::UnitId>, Vec2)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        eprintln!("skip: {} missing", path.display());
        return None;
    }
    let base = map_mpq_base(&path).expect("mpq base");
    let archive = MpqArchive::open_at(&path, base).expect("mpq");
    let wpm = PathingMap::parse(&archive.read("war3map.wpm").expect("wpm")).expect("wpm parse");
    let units = parse_units(&archive.read("war3mapUnits.doo").expect("units")).expect("units parse");
    let doodads =
        parse_doodads(&archive.read("war3map.doo").expect("doodads")).expect("doodads parse");
    let terrain =
        wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").expect("w3e")).expect("w3e");
    let origin = Vec2::new(terrain.center.0, terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);

    let mut sim = Sim::new(0x5EED, wpm.width, wpm.height);
    sim.passive_income = false;
    let mut walkable = Vec::with_capacity(wpm.width as usize * wpm.height as usize);
    for y in 0..wpm.height {
        for x in 0..wpm.width {
            walkable.push(wpm.walkable(x, y));
        }
    }
    sim.set_pathing(wpm.width, wpm.height, wc3_sim::PATHING_CELL_SIZE, walkable);
    for doodad in &doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        sim.spawn_destructable(&id, to_world(doodad.position.x, doodad.position.y));
    }
    for unit in &units {
        if &unit.type_id == b"ngol" || &unit.type_id == b"ugol" {
            sim.add_gold_mine(to_world(unit.position.x, unit.position.y), wc3_sim::GOLD_MINE_GOLD);
        }
    }
    let start0 = units
        .iter()
        .find(|unit| &unit.type_id == b"sloc")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .expect("start location");
    // The game's melee opening: hall near the start, five workers on a ring.
    let hall = sim
        .spawn_def(0, hall_for_race(Race::Human), sim.walkable_pos_near(start0, 24).unwrap_or(start0))
        .expect("hall");
    let hall_pos = sim.unit(hall).unwrap().pos;
    let mut workers = Vec::new();
    for slot in 0..5usize {
        let angle = std::f32::consts::TAU * slot as f32 / 5.0;
        let wanted = hall_pos + Vec2::new(angle.cos(), angle.sin()) * 300.0;
        if let Some(pos) = sim.walkable_pos_near(wanted, 16)
            && let Some(id) = sim.spawn_def(0, worker_for_race(Race::Human), pos)
        {
            workers.push(id);
        }
    }
    assert_eq!(workers.len(), 5, "five starting workers");
    let mine = sim
        .nearest_gold_mine(sim.unit(workers[0]).unwrap().pos)
        .expect("nearest gold mine");
    Some((sim, workers, mine))
}

fn dump(sim: &Sim, workers: &[wc3_sim::UnitId], tag: &str) {
    for &id in workers {
        let Some(unit) = sim.unit(id) else { continue };
        let (order, phase, timer) = match unit.order {
            Order::Harvest { phase, .. } => ("Harvest".to_string(), phase, unit.harvest_timer),
            Order::HarvestLumber { phase, .. } => {
                ("HarvestLumber".to_string(), phase, unit.harvest_timer)
            }
            other => (format!("{other:?}"), HarvestPhase::ToMine, 0),
        };
        eprintln!(
            "MULTIDBG {tag} tick={} unit={} pos=({:.0},{:.0}) order={} phase={:?} path_len={} path_idx={} goal={:?} mine={:?} carry={} timer={} stuck={}",
            sim.tick,
            id.0,
            unit.pos.x,
            unit.pos.y,
            order,
            phase,
            unit.path.len(),
            unit.path_index,
            unit.goal.map(|p| (p.x.round() as i32, p.y.round() as i32)),
            match unit.order {
                Order::Harvest { mine, .. } => Some((mine.x.round() as i32, mine.y.round() as i32)),
                _ => None,
            },
            unit.carrying,
            timer,
            unit.stuck_ticks,
        );
    }
}

/// The exact geometry the game's melee opening produced on Justice (probe
/// `analysis/agent-sim/probe`, ledger 414): hall (14256,1968), the five worker
/// ring positions, and the mine `nearest_gold_mine` selected (14576,1808).
/// This is the layout where one worker used to drop to `Idle` while the other
/// four kept paying.
fn game_geometry(sim: &mut Sim) -> (Vec<wc3_sim::UnitId>, Vec2) {
    let hall = sim.spawn_def(0, "htow", Vec2::new(14256.0, 1968.0)).expect("hall");
    assert!(sim.unit(hall).is_some());
    let ring = [
        Vec2::new(14608.0, 2000.0),
        Vec2::new(14384.0, 2320.0),
        Vec2::new(14032.0, 2192.0),
        Vec2::new(14032.0, 1808.0),
        Vec2::new(14384.0, 1680.0),
    ];
    let mut workers = Vec::new();
    for (index, pos) in ring.iter().enumerate() {
        let pos = sim.walkable_pos_near(*pos, 16).unwrap_or(*pos);
        workers.push(
            sim.spawn_def(0, "hpea", pos)
                .unwrap_or_else(|| panic!("worker {index} at {pos:?}")),
        );
    }
    let mine = Vec2::new(14576.0, 1808.0);
    sim.add_gold_mine(mine, wc3_sim::GOLD_MINE_GOLD);
    (workers, mine)
}

/// The game builds its grid out of terrain tiles (`TERRAIN_TILE_SIZE = 128`)
/// rather than the map's native 32-unit WPM cells, so every footprint and
/// arrival gate is four times coarser than in the sim tests above.  This test
/// reproduces the *game* cell size with the same Justice geometry: a worker
/// must still never end up frozen on an unstandable cell with its gather order
/// dropped.
#[test]
fn economy_gold_coarse_game_grid_five_workers_all_pay() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        eprintln!("skip: {} missing", path.display());
        return;
    }
    let base = map_mpq_base(&path).expect("mpq base");
    let archive = MpqArchive::open_at(&path, base).expect("mpq");
    let wpm = PathingMap::parse(&archive.read("war3map.wpm").expect("wpm")).expect("wpm parse");
    let doodads =
        parse_doodads(&archive.read("war3map.doo").expect("doodads")).expect("doodads parse");
    let terrain =
        wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").expect("w3e")).expect("w3e");
    let origin = Vec2::new(terrain.center.0, terrain.center.1);
    // Terrain-tile grid exactly like `wc3_game::game::build_grid`: sample the
    // WPM at the centre of each 128-unit tile.
    let tile = 128.0f32;
    let width = (wpm.width as f32 / 4.0).ceil() as u32;
    let height = (wpm.height as f32 / 4.0).ceil() as u32;
    let sx = wpm.width as f32 / width as f32;
    let sy = wpm.height as f32 / height as f32;
    let mut walkable = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let px = ((x as f32 + 0.5) * sx) as u32;
            let py = ((y as f32 + 0.5) * sy) as u32;
            walkable.push(wpm.walkable(px.min(wpm.width - 1), py.min(wpm.height - 1)));
        }
    }
    let mut sim = Sim::new(0x5EED, width, height);
    sim.passive_income = false;
    sim.set_pathing(width, height, tile, walkable);
    for doodad in &doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        sim.spawn_destructable(
            &id,
            Vec2::new(doodad.position.x - origin.x, doodad.position.y - origin.y),
        );
    }
    let (workers, mine) = game_geometry(&mut sim);
    let hall = sim
        .alive_units()
        .find(|unit| unit.owner == 0 && unit.def().main_hall)
        .map(|unit| unit.id)
        .expect("hall");
    eprintln!(
        "COARSE setup hall_pos={:?} blocked={:?} cell={:?} walkable={:?}",
        sim.unit(hall).map(|unit| unit.pos),
        sim.unit(hall).map(|unit| unit.blocked_cells.clone()),
        sim.unit(hall).map(|unit| sim.grid.world_to_cell(unit.pos)),
        sim.unit(hall)
            .map(|unit| {
                let cell = sim.grid.world_to_cell(unit.pos);
                sim.grid.walkable(cell.0, cell.1)
            })
    );
    let before = sim.resources[0];
    let mut trips = vec![0usize; workers.len()];
    let mut seen_carry = vec![false; workers.len()];
    sim.step(&[Command::Harvest {
        units: workers.clone(),
        to: mine,
    }]);
    eprintln!(
        "COARSE after1 hall_pos={:?} worker_cells={:?}",
        sim.unit(hall).map(|unit| unit.pos),
        workers
            .iter()
            .map(|id| sim.unit(*id).map(|unit| {
                let cell = sim.grid.world_to_cell(unit.pos);
                (cell, sim.grid.walkable(cell.0, cell.1))
            }))
            .collect::<Vec<_>>()
    );
    let mut first_error: Option<(u64, &'static str)> = None;
    for _ in 0..2400 {
        sim.step(&[]);
        for event in &sim.events {
            if let wc3_sim::Event::Error(text) = event {
                first_error.get_or_insert((sim.tick, text));
            }
        }
        for (slot, &id) in workers.iter().enumerate() {
            let Some(unit) = sim.unit(id) else { continue };
            if unit.carrying > 0 && !seen_carry[slot] {
                seen_carry[slot] = true;
                trips[slot] += 1;
            } else if unit.carrying == 0 {
                seen_carry[slot] = false;
            }
        }
    }
    let gained = sim.resources[0] - before;
    eprintln!(
        "COARSEGEOM gained={gained} trips={trips:?} error={first_error:?} states={:?}",
        workers
            .iter()
            .map(|id| sim.unit(*id).map(|unit| (unit.order, unit.pos)))
            .collect::<Vec<_>>()
    );
    assert!(gained > 0, "the crew pays on the coarse game grid");
    for (slot, &id) in workers.iter().enumerate() {
        assert!(
            trips[slot] > 0,
            "worker {slot} ({id:?}) never picked up a load on the game grid: {:?}",
            sim.unit(id).map(|unit| (unit.order, unit.pos, unit.stuck_ticks))
        );
    }
}

#[test]
fn economy_gold_game_opening_five_workers_all_pay() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        eprintln!("skip: {} missing", path.display());
        return;
    }
    let base = map_mpq_base(&path).expect("mpq base");
    let archive = MpqArchive::open_at(&path, base).expect("mpq");
    let wpm = PathingMap::parse(&archive.read("war3map.wpm").expect("wpm")).expect("wpm parse");
    let doodads =
        parse_doodads(&archive.read("war3map.doo").expect("doodads")).expect("doodads parse");
    let terrain =
        wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").expect("w3e")).expect("w3e");
    let origin = Vec2::new(terrain.center.0, terrain.center.1);
    let mut sim = Sim::new(0x5EED, wpm.width, wpm.height);
    sim.passive_income = false;
    let mut walkable = Vec::with_capacity(wpm.width as usize * wpm.height as usize);
    for y in 0..wpm.height {
        for x in 0..wpm.width {
            walkable.push(wpm.walkable(x, y));
        }
    }
    sim.set_pathing(wpm.width, wpm.height, wc3_sim::PATHING_CELL_SIZE, walkable);
    for doodad in &doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        sim.spawn_destructable(
            &id,
            Vec2::new(doodad.position.x - origin.x, doodad.position.y - origin.y),
        );
    }
    let (workers, mine) = game_geometry(&mut sim);
    let before = sim.resources[0];
    let mut trips = vec![0usize; workers.len()];
    let mut seen_carry = vec![false; workers.len()];
    sim.step(&[Command::Harvest {
        units: workers.clone(),
        to: mine,
    }]);
    for _ in 0..2400 {
        sim.step(&[]);
        for (slot, &id) in workers.iter().enumerate() {
            let Some(unit) = sim.unit(id) else { continue };
            if unit.carrying > 0 && !seen_carry[slot] {
                seen_carry[slot] = true;
                trips[slot] += 1;
            } else if unit.carrying == 0 {
                seen_carry[slot] = false;
            }
        }
    }
    let gained = sim.resources[0] - before;
    eprintln!(
        "GAMEGEOM gained={gained} trips={trips:?} orders={:?}",
        workers
            .iter()
            .map(|id| sim.unit(*id).map(|unit| unit.order))
            .collect::<Vec<_>>()
    );
    assert!(gained > 0, "the crew pays");
    for (slot, &id) in workers.iter().enumerate() {
        assert!(
            trips[slot] > 0,
            "worker {slot} ({id:?}) never picked up a load: {:?}",
            sim.unit(id).map(|unit| (unit.order, unit.pos, unit.stuck_ticks))
        );
        assert!(
            !matches!(sim.unit(id).map(|unit| unit.order), Some(Order::Idle)),
            "worker {slot} ended Idle instead of harvesting: {:?}",
            sim.unit(id).map(|unit| (unit.pos, unit.stuck_ticks))
        );
    }
}

#[test]
fn economy_gold_five_workers_one_mine_on_justice_all_pay() {
    let Some((mut sim, workers, mine)) = justice_sim() else {
        return;
    };
    let mine_remaining = sim
        .gold_mines
        .iter()
        .find(|entry| entry.pos == mine)
        .map(|entry| entry.remaining)
        .unwrap_or(-1);
    let before = sim.resources[0];
    sim.step(&[Command::Harvest {
        units: workers.clone(),
        to: mine,
    }]);
    dump(&sim, &workers, "order");
    let mut last_gold = before;
    let mut deliveries = 0usize;
    for frame in 0..2400u32 {
        sim.step(&[]);
        if frame % 60 == 59 {
            dump(&sim, &workers, "t");
            if sim.resources[0] != last_gold {
                deliveries += 1;
                last_gold = sim.resources[0];
            }
        }
    }
    let gained = sim.resources[0] - before;
    eprintln!(
        "MULTIDBG summary gold={before}->{} gained={gained} mine_remaining={} ordered={mine_remaining} deliveries_seen={deliveries} mine={:?}",
        sim.resources[0],
        sim
            .gold_mines
            .iter()
            .find(|entry| entry.pos == mine)
            .map(|entry| entry.remaining)
            .unwrap_or(-1),
        mine,
    );
    assert!(
        gained > 0,
        "five workers ordered to one mine must pay (gained {gained})"
    );
    assert!(
        gained % 10 == 0,
        "each delivered load is exactly ten gold (gained {gained})"
    );
}
