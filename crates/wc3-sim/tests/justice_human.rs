//! mechanic `gameplay.justice-human`: a human-command scenario on
//! `(2)Justice.w3x` — harvest, train, build, attack, win.  Every order below is
//! a player command (`Command::*`), never an AI decision.

mod common;

use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_doodads, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::{Command, Event, Race, Sim, UnitId, Vec2, data, def_of};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

struct Justice {
    wpm: PathingMap,
    terrain: wc3_formats::w3e::Terrain,
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
    Some(Justice {
        wpm: PathingMap::parse(&archive.read("war3map.wpm").ok()?).ok()?,
        terrain: wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").ok()?).ok()?,
        units: parse_units(&archive.read("war3mapUnits.doo").ok()?).ok()?,
        doodads: parse_doodads(&archive.read("war3map.doo").ok()?).ok()?,
    })
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

/// mechanic `gameplay.justice-human`: a scripted player wins a real match on the
/// map with harvest → train → build → attack orders only.
#[test]
fn gameplay_justice_human_scenario_harvest_train_build_attack_win() {
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
    assert_eq!(starts.len(), 2);

    let mut sim = Sim::new(0x3822, map.wpm.width, map.wpm.height);
    sim.passive_income = false;
    sim.resources = [1200, 1200];
    sim.lumber = [500, 500];
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
    for doodad in &map.doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        sim.spawn_destructable(&id, to_world(doodad.position.x, doodad.position.y));
    }
    let mines: Vec<Vec2> = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"ngol")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    assert!(!mines.is_empty(), "Justice has gold mines");
    for mine in &mines {
        sim.add_gold_mine(*mine, wc3_sim::GOLD_MINE_GOLD);
    }

    // Human (player 0) HQ; enemy (player 1) HQ is spawned but never gets an AI:
    // every hostile action is a player order.
    let player_base = starts[0];
    let enemy_base = starts[1];
    let nearest_mine = |base: Vec2| {
        *mines
            .iter()
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
    let player_mine = nearest_mine(player_base);
    let enemy_mine = nearest_mine(enemy_base);
    let player_tree = tree_near(&sim, player_base);
    let enemy_tree = tree_near(&sim, enemy_base);

    let hall = sim
        .spawn_def_exact(
            0,
            "htow",
            free_site(&sim, player_base, 3).unwrap_or(player_base),
        )
        .expect("hall");
    let mut workers = Vec::new();
    for slot in 0..5 {
        let angle = slot as f32 * std::f32::consts::TAU / 5.0;
        let wanted = sim.unit(hall).unwrap().pos + Vec2::new(angle.cos(), angle.sin()) * 300.0;
        if let Some(pos) = free_site(&sim, wanted, 1)
            && let Some(worker) = sim.spawn_def_exact(0, "hpea", pos)
        {
            workers.push(worker);
        }
    }
    let enemy_hall = sim
        .spawn_def_exact(
            1,
            "ogre",
            free_site(&sim, enemy_base, 3).unwrap_or(enemy_base),
        )
        .expect("enemy hall");
    for slot in 0..4 {
        let angle = slot as f32 * std::f32::consts::TAU / 3.0;
        let wanted =
            sim.unit(enemy_hall).unwrap().pos + Vec2::new(angle.cos(), angle.sin()) * 260.0;
        if let Some(pos) = free_site(&sim, wanted, 1) {
            sim.spawn_def_exact(1, "opeo", pos);
        }
    }

    // 1. Harvest: player orders every worker to the mine, one to the trees.
    let lumber_worker = workers[1];
    sim.step(&[Command::Harvest {
        units: workers.clone(),
        to: player_mine,
    }]);
    let _ = sim.harvest_lumber(&[lumber_worker], player_tree);
    assert!(matches!(
        sim.unit(workers[0]).unwrap().order,
        wc3_sim::Order::Harvest { .. } | wc3_sim::Order::Move(_) | wc3_sim::Order::Idle
    ));

    // 2. Build: supply and barracks through player build orders.
    let builder = workers[0];
    let mut supply = None;
    for slot in 0..3 {
        let angle = slot as f32 * std::f32::consts::TAU / 3.0;
        let site = free_site(
            &sim,
            player_base + Vec2::new(angle.cos(), angle.sin()) * 460.0,
            2,
        )
        .expect("supply site");
        let farm_builder = workers[slot % workers.len()];
        let farm = sim
            .place_building_ordered(farm_builder, "hhou", site)
            .expect("farm");
        supply.get_or_insert(farm);
    }
    let supply = supply.expect("farm");
    let barracks_site =
        free_site(&sim, player_base + Vec2::new(-420.0, 160.0), 3).expect("barracks site");
    let barracks = sim
        .place_building_ordered(builder, "hbar", barracks_site)
        .expect("barracks");

    // Let the ordered farms/barracks finish before training (real build time).
    for _ in 0..16_000 {
        sim.step(&[]);
        let farms_ready = sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 0 && unit.def().id == "hhou")
            .filter(|unit| unit.construct.is_none())
            .count();
        if farms_ready >= 4
            && sim
                .unit(barracks)
                .map(|unit| unit.construct.is_none())
                .unwrap_or(false)
        {
            break;
        }
    }

    // 3. Train: player queues footmen at the barracks and a worker at the hall.
    let mut trained = 0usize;
    let mut footmen = 0usize;
    let mut gold_seen = 0usize;
    for _ in 0..20_000 {
        sim.step(&[]);
        for event in &sim.events {
            match event {
                Event::GoldDeposited { owner, .. } if *owner == 0 => gold_seen += 1,
                Event::UnitTrained(id) => {
                    if sim.unit(*id).map(|unit| unit.owner == 0).unwrap_or(false) {
                        trained += 1;
                        if sim
                            .unit(*id)
                            .map(|unit| unit.def().id == "hfoo")
                            .unwrap_or(false)
                        {
                            footmen += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        if sim
            .unit(barracks)
            .map(|unit| unit.construct.is_none())
            .unwrap_or(false)
        {
            let queued = sim.unit(barracks).unwrap().production.len();
            if queued == 0 {
                let _ = sim.train_def(barracks, "hfoo");
            }
        }
        if sim
            .unit(hall)
            .map(|unit| unit.construct.is_none())
            .unwrap_or(false)
            && sim.unit(hall).unwrap().production.is_empty()
            && sim.supply_used(0) + 2 <= sim.supply_cap(0)
            && footmen == 0
        {
            let _ = sim.train_def(hall, "hpea");
        }
        if footmen >= 4 {
            break;
        }
    }
    assert!(gold_seen > 0, "worker delivered gold");
    assert!(trained >= 4, "units trained by player orders: {trained}");
    assert!(supply != barracks);

    // 4. Attack: player selects the army and attack-moves the enemy hall.
    let army: Vec<UnitId> = sim
        .units
        .iter()
        .filter(|unit| {
            unit.alive && unit.owner == 0 && !unit.is_building() && unit.def().id == "hfoo"
        })
        .map(|unit| unit.id)
        .collect();
    assert!(army.len() >= 4, "army assembled: {}", army.len());
    let enemy_hall_pos = sim.unit(enemy_hall).unwrap().pos;
    sim.step(&[Command::MoveAttack {
        units: army.clone(),
        to: enemy_hall_pos,
    }]);

    // 5. Win: keep re-issuing the attack as units die until the enemy hall dies.
    let mut enemy_hall_died = false;
    for _ in 0..18_000 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::UnitDied(id) = event
                && *id == enemy_hall
            {
                enemy_hall_died = true;
            }
        }
        if enemy_hall_died {
            break;
        }
        if sim.tick % 60 == 0 {
            let alive: Vec<UnitId> = army
                .iter()
                .copied()
                .filter(|id| sim.unit(*id).map(|unit| unit.alive).unwrap_or(false))
                .collect();
            if !alive.is_empty() {
                let target = sim
                    .unit(enemy_hall)
                    .map(|unit| unit.pos)
                    .unwrap_or(enemy_hall_pos);
                sim.step(&[Command::MoveAttack {
                    units: alive,
                    to: target,
                }]);
            } else {
                break;
            }
        }
    }
    assert!(enemy_hall_died, "player army destroyed the enemy hall");

    // Victory is evaluated from the remaining units, exactly as for a human.
    let mut winner = None;
    for _ in 0..3 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::Victory(owner) = event {
                winner = Some(*owner);
            }
        }
    }
    // The enemy workers survive, so victory needs their removal as well: the
    // player keeps hunting them, which is the normal "last man standing" rule.
    if winner.is_none() {
        let remaining: Vec<UnitId> = sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 1)
            .map(|unit| unit.id)
            .collect();
        if !remaining.is_empty() {
            let hunters: Vec<UnitId> = sim
                .units
                .iter()
                .filter(|unit| {
                    unit.alive && unit.owner == 0 && !unit.is_building() && unit.damage > 0
                })
                .map(|unit| unit.id)
                .collect();
            if !hunters.is_empty() {
                let target = sim
                    .unit(remaining[0])
                    .map(|unit| unit.pos)
                    .unwrap_or(enemy_base);
                sim.step(&[Command::MoveAttack {
                    units: hunters,
                    to: target,
                }]);
                for _ in 0..9_000 {
                    sim.step(&[]);
                    for event in &sim.events {
                        if let Event::Victory(owner) = event {
                            winner = Some(*owner);
                        }
                    }
                    if winner.is_some() {
                        break;
                    }
                    if sim.tick % 120 == 0 {
                        let alive: Vec<UnitId> = sim
                            .units
                            .iter()
                            .filter(|unit| unit.alive && unit.owner == 1)
                            .map(|unit| unit.id)
                            .collect();
                        let hunters: Vec<UnitId> = sim
                            .units
                            .iter()
                            .filter(|unit| {
                                unit.alive
                                    && unit.owner == 0
                                    && !unit.is_building()
                                    && unit.damage > 0
                            })
                            .map(|unit| unit.id)
                            .collect();
                        if alive.is_empty() || hunters.is_empty() {
                            break;
                        }
                        let target = sim
                            .unit(alive[0])
                            .map(|unit| unit.pos)
                            .unwrap_or(enemy_base);
                        sim.step(&[Command::MoveAttack {
                            units: hunters,
                            to: target,
                        }]);
                    }
                }
            }
        }
    }
    assert_eq!(
        winner,
        Some(0),
        "player wins the scenario (winner={winner:?})"
    );
    let _ = (HallReport {
        hall,
        supply,
        enemy_mine,
        enemy_tree,
    });
}

#[allow(dead_code)]
struct HallReport {
    hall: UnitId,
    supply: UnitId,
    enemy_mine: Vec2,
    enemy_tree: Vec2,
}
