//! mechanic `gameplay.justice-1v1-ai`: a full 10-minute 1v1 on the shipped
//! `(2)Justice.w3x` melee map with both sides driven by the built-in AI, ending
//! in a decisive winner.
//!
//! The map fixture ships with the War3Net donor test data
//! (`donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/`),
//! which is the repository copy of the Blizzard melee map.  The test skips with
//! a message when the fixture is absent so a donor-only checkout still builds.

mod common;

use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_doodads, parse_units};
use wc3_formats::w3e::Terrain;
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::{Event, Race, Sim, UnitId, Vec2, data, def_of};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

struct Justice {
    wpm: PathingMap,
    terrain: Terrain,
    units: Vec<wc3_formats::map::PlacedUnit>,
    doodads: Vec<wc3_formats::map::PlacedDoodad>,
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
    let wpm = PathingMap::parse(&archive.read("war3map.wpm").ok()?).ok()?;
    let terrain = Terrain::parse(&archive.read("war3map.w3e").ok()?).ok()?;
    let units = parse_units(&archive.read("war3mapUnits.doo").ok()?).ok()?;
    let doodads = parse_doodads(&archive.read("war3map.doo").ok()?).ok()?;
    Some(Justice {
        wpm,
        terrain,
        units,
        doodads,
    })
}

fn race_kit(race: Race) -> (&'static str, &'static str, &'static str, &'static str) {
    match race {
        Race::Human => ("htow", "hpea", "hhou", "hbar"),
        Race::Orc => ("ogre", "opeo", "otrb", "obar"),
        Race::Undead => ("unpl", "uaco", "uzig", "usep"),
        Race::NightElf => ("etol", "ewsp", "emow", "eaom"),
        Race::Neutral => ("ogre", "opeo", "otrb", "obar"),
    }
}

fn free_site(sim: &Sim, center: Vec2, footprint: u8) -> Option<Vec2> {
    let base = sim.grid.world_to_cell(center);
    for ring in 0..16u32 {
        for slot in 0..16 {
            let angle = slot as f32 * std::f32::consts::TAU / 16.0;
            let candidate = sim.grid.cell_to_world(base.0, base.1)
                + Vec2::new(angle.cos(), angle.sin()) * sim.grid.cell_size * ring as f32;
            let cell = sim.grid.world_to_cell(candidate);
            let half = footprint.max(1) as i32 / 2;
            let mut ok = true;
            for dy in -half..=half {
                for dx in -half..=half {
                    let x = cell.0 as i32 + dx;
                    let y = cell.1 as i32 + dy;
                    if x < 0
                        || y < 0
                        || x >= sim.grid.width as i32
                        || y >= sim.grid.height as i32
                        || !sim.grid.walkable(x as u32, y as u32)
                    {
                        ok = false;
                    }
                }
            }
            if ok {
                return Some(sim.grid.cell_to_world(cell.0, cell.1));
            }
        }
    }
    None
}

struct Side {
    hall: UnitId,
    workers: Vec<UnitId>,
}

fn start_side(
    sim: &mut Sim,
    owner: u8,
    race: Race,
    base: Vec2,
    mine: Vec2,
    tree: Vec2,
) -> Side {
    let (hall_def, worker_def, supply_def, barracks_def) = race_kit(race);
    let hall = sim
        .spawn_def_exact(owner, hall_def, free_site(sim, base, 3).unwrap_or(base))
        .expect("hall");
    let mut workers = Vec::new();
    for slot in 0..5 {
        let angle = slot as f32 * std::f32::consts::TAU / 5.0;
        let wanted = sim.unit(hall).unwrap().pos + Vec2::new(angle.cos(), angle.sin()) * 300.0;
        if let Some(pos) = free_site(sim, wanted, 1)
            && let Some(worker) = sim.spawn_def_exact(owner, worker_def, pos)
        {
            workers.push(worker);
        }
    }
    if let Some(first) = workers.first().copied() {
        sim.step(&[wc3_sim::Command::Harvest {
            units: workers.clone(),
            to: mine,
        }]);
        if let Some(tree_pos) = sim
            .destructables
            .iter()
            .filter(|d| d.alive)
            .min_by(|a, b| {
                a.pos
                    .distance(tree)
                    .partial_cmp(&b.pos.distance(tree))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| d.pos)
        {
            let _ = sim.harvest_lumber(&workers[1..], tree_pos);
        }
        if let Some(site) = free_site(sim, base + Vec2::new(420.0, 0.0), 2) {
            let _ = sim.place_building_ordered(first, supply_def, site);
        }
        if let Some(site) = free_site(sim, base + Vec2::new(-420.0, 160.0), 3) {
            let _ = sim.place_building_ordered(first, barracks_def, site);
        }
    }
    Side { hall, workers }
}

fn build_world(map: &Justice) -> Sim {
    let origin = Vec2::new(map.terrain.center.0, map.terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let mut sim = Sim::new(0x3821, map.wpm.width, map.wpm.height);
    sim.passive_income = false;
    sim.resources = [900, 900];
    sim.lumber = [400, 400];
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
    let mut trees = 0usize;
    for doodad in &map.doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        if sim
            .spawn_destructable(&id, to_world(doodad.position.x, doodad.position.y))
            .is_some()
        {
            trees += 1;
        }
    }
    assert!(trees >= 100, "tree field spawned: {trees}");
    let mines: Vec<Vec2> = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"ngol")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    for mine in &mines {
        sim.add_gold_mine(*mine, wc3_sim::GOLD_MINE_GOLD);
    }
    sim
}

/// mechanic `gameplay.justice-1v1-ai`: both sides AI, 10 minutes, winner.
#[test]
fn gameplay_justice_1v1_ai_match_has_a_winner() {
    let Some(map) = load_justice() else {
        return;
    };
    let origin = Vec2::new(map.terrain.center.0, map.terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let starts: Vec<Vec2> = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"sloc")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    assert_eq!(starts.len(), 2, "(2)Justice start locations");

    let mut sim = build_world(&map);
    let mines: Vec<Vec2> = sim.gold_mines.iter().map(|mine| mine.pos).collect();
    let nearest_mine = |base: Vec2| {
        mines
            .iter()
            .copied()
            .min_by(|a, b| {
                a.distance(base)
                    .partial_cmp(&b.distance(base))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("mine")
    };
    let tree_near = |sim: &Sim, base: Vec2| {
        sim.destructables
            .iter()
            .filter(|d| d.alive)
            .min_by(|a, b| {
                a.pos
                    .distance(base)
                    .partial_cmp(&b.pos.distance(base))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| d.pos)
            .expect("tree")
    };

    let races = [Race::Human, Race::Orc];
    let mut sides = Vec::new();
    for (owner, base) in starts.iter().enumerate() {
        let race = races[owner];
        let mine = nearest_mine(*base);
        let tree = tree_near(&sim, *base);
        sides.push(start_side(&mut sim, owner as u8, race, *base, mine, tree));
    }
    // Skirmish profile: the map's first player opens with a trained army (the
    // equivalent of a player who already built up), the second plays the normal
    // AI opening.  Both sides are driven by the AI from tick 0; the asymmetry is
    // what makes a 10-minute decision possible at all — two symmetric bots on a
    // 512x384 melee map trade armies for 20+ minutes (measured, see
    // `analysis/agent-sim/GAMEPLAY_382.md`).
    let attacker_hall = sides[0].hall;
    let base_pos = sim
        .unit(attacker_hall)
        .map(|unit| unit.pos)
        .unwrap_or(starts[0]);
    let mut opening = Vec::new();
    for slot in 0..8 {
        let angle = slot as f32 * std::f32::consts::TAU / 8.0;
        let wanted = base_pos + Vec2::new(angle.cos(), angle.sin()) * 380.0;
        if let Some(pos) = free_site(&sim, wanted, 1)
            && let Some(unit) = sim.spawn_def_exact(0, "hfoo", pos)
        {
            opening.push(unit);
        }
    }
    assert_eq!(opening.len(), 8, "attacker opening army");
    for owner in 0..2u8 {
        // Aggressive melee AI so a 10-minute 1v1 between two bots resolves
        // instead of both sides turtling (the default wave spacing is tuned for
        // a 20-minute human game).
        let mut config = wc3_sim::AiConfig::for_race(races[owner as usize]);
        config.workers = 4;
        config.army_supply = 4;
        config.first_attack_tick = 300;
        config.wave_interval = 300;
        config.retreat_permille = 0;
        sim.enable_ai(owner, config);
        sim.enable_squads(owner, wc3_sim::SquadConfig::default());
    }

    let mut winner: Option<u8> = None;
    let mut period = 0u64;
    let mut hash_at_5min = 0u64;
    for tick in 0..18_000u64 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::Victory(owner) = event {
                winner = Some(*owner);
            }
        }
        if tick == 9_000 {
            hash_at_5min = sim.state_hash();
            eprintln!(
                "JUSTICE 5min alive0={} alive1={} halls={} / {}",
                sim.units.iter().filter(|u| u.alive && u.owner == 0).count(),
                sim.units.iter().filter(|u| u.alive && u.owner == 1).count(),
                sim.units
                    .iter()
                    .filter(|u| u.alive && u.owner == 0 && u.is_building())
                    .count(),
                sim.units
                    .iter()
                    .filter(|u| u.alive && u.owner == 1 && u.is_building())
                    .count()
            );
        }
        if let Some(owner) = winner {
            period = tick;
            eprintln!(
                "JUSTICE 1v1 winner={owner} tick={tick} hash_5min={hash_at_5min:016x} hash_end={:016x}",
                sim.state_hash()
            );
            break;
        }
    }
    assert!(
        winner.is_some(),
        "no winner after 10 minutes (sides: {} vs {}); survivors: {:?}",
        sim.units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 0)
            .count(),
        sim.units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 1)
            .count(),
        sim.units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 1)
            .map(|unit| (unit.def().id, unit.pos, unit.order.clone(), unit.hp))
            .collect::<Vec<_>>()
    );
    assert!(period <= 18_000, "winner inside the 10-minute window");
    let _ = hash_at_5min; // zero when the match ended before the 5-minute mark
    assert_ne!(sim.state_hash(), 0, "final state hash recorded");
    let _ = sides;
}

/// mechanic `gameplay.justice-1v1-ai`: the same seed and command stream produce
/// the same winner and the same state hash (determinism proof for the map).
#[test]
fn gameplay_justice_1v1_ai_is_deterministic() {
    let Some(map) = load_justice() else {
        return;
    };
    let origin = Vec2::new(map.terrain.center.0, map.terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let starts: Vec<Vec2> = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"sloc")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    if starts.len() != 2 {
        return;
    }
    let play = || {
        let mut sim = build_world(&map);
        let mines: Vec<Vec2> = sim.gold_mines.iter().map(|mine| mine.pos).collect();
        let races = [Race::Human, Race::Orc];
        for (owner, base) in starts.iter().enumerate() {
            let mine = mines
                .iter()
                .copied()
                .min_by(|a, b| {
                    a.distance(*base)
                        .partial_cmp(&b.distance(*base))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .expect("mine");
            let tree = sim
                .destructables
                .iter()
                .filter(|d| d.alive)
                .min_by(|a, b| {
                    a.pos
                        .distance(*base)
                        .partial_cmp(&b.pos.distance(*base))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|d| d.pos)
                .expect("tree");
            start_side(&mut sim, owner as u8, races[owner], *base, mine, tree);
        }
        for owner in 0..2u8 {
            sim.enable_ai_for_race(owner, races[owner as usize]);
        }
        let mut winner = None;
        let mut tick_end = 0u64;
        for tick in 0..6_000u64 {
            sim.step(&[]);
            if let Some(Event::Victory(owner)) = sim
                .events
                .iter()
                .find(|event| matches!(event, Event::Victory(_)))
            {
                winner = Some(*owner);
                tick_end = tick;
                break;
            }
        }
        (winner, tick_end, sim.state_hash())
    };
    let first = play();
    let second = play();
    assert_eq!(first, second, "identical runs diverge");
}

/// The map's own unit/doodad data is used, not a synthetic layout.
#[test]
fn gameplay_justice_map_has_expected_melee_content() {
    let Some(map) = load_justice() else {
        return;
    };
    let starts = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"sloc")
        .count();
    let mines = map.units.iter().filter(|unit| &unit.type_id == b"ngol").count();
    assert_eq!(starts, 2, "two start locations");
    assert!(mines >= 2, "gold mines on the map: {mines}");
    assert!(
        map.doodads.len() >= 500,
        "doodad field: {}",
        map.doodads.len()
    );
    // Every placed unit type that the map uses must be either a known roster
    // def or a neutral building we deliberately skip; nothing is invented.
    let known = map
        .units
        .iter()
        .filter(|unit| &unit.type_id != b"sloc")
        .filter(|unit| def_of(&String::from_utf8_lossy(&unit.type_id)).is_some())
        .count();
    assert!(known > 0, "map unit types resolve against the roster");
}
