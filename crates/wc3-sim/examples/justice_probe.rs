//! Diagnostic probe for the (2)Justice 1v1 AI match: prints unit counts, tree
//! count and per-second timing so a slow or stuck match is visible without
//! reading a test log.
//!
//!   cargo run -p wc3-sim --example justice_probe -- [ticks]

use std::path::Path;
use std::time::Instant;

use wc3_formats::map::{map_mpq_base, parse_doodads, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::{Race, Sim, Vec2, data};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

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

fn main() {
    let ticks: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(6_000);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    let base = map_mpq_base(&path).expect("mpq base");
    let archive = MpqArchive::open_at(&path, base).expect("mpq");
    let wpm = PathingMap::parse(&archive.read("war3map.wpm").expect("wpm")).expect("wpm parse");
    let units = parse_units(&archive.read("war3mapUnits.doo").expect("units")).expect("units");
    let doodads = parse_doodads(&archive.read("war3map.doo").expect("doodads")).expect("doo");

    let terrain = wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").expect("w3e"))
        .expect("w3e parse");
    let origin = Vec2::new(terrain.center.0, terrain.center.1);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);

    let mut sim = Sim::new(0x3821, wpm.width, wpm.height);
    sim.passive_income = false;
    sim.resources = [900, 900];
    sim.lumber = [400, 400];
    let mut walkable = Vec::with_capacity(wpm.width as usize * wpm.height as usize);
    for y in 0..wpm.height {
        for x in 0..wpm.width {
            walkable.push(wpm.walkable(x, y));
        }
    }
    sim.set_pathing(
        wpm.width,
        wpm.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );
    let mut trees = 0usize;
    let mut rejected = 0usize;
    for doodad in &doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        if sim
            .spawn_destructable(&id, to_world(doodad.position.x, doodad.position.y))
            .is_some()
        {
            trees += 1;
        } else {
            rejected += 1;
        }
    }
    println!("map {}x{} doodads={} trees={trees} rejected={rejected}", wpm.width, wpm.height, doodads.len());
    let mines: Vec<Vec2> = units
        .iter()
        .filter(|unit| &unit.type_id == b"ngol")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    for mine in &mines {
        sim.add_gold_mine(*mine, wc3_sim::GOLD_MINE_GOLD);
    }
    let starts: Vec<Vec2> = units
        .iter()
        .filter(|unit| &unit.type_id == b"sloc")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    let races = [Race::Human, Race::Orc];
    for (owner, base) in starts.iter().enumerate() {
        let (hall_def, worker_def, supply_def, barracks_def) = race_kit(races[owner]);
        let hall = sim
            .spawn_def_exact(owner as u8, hall_def, free_site(&sim, *base, 3).unwrap_or(*base))
            .expect("hall");
        let mut workers = Vec::new();
        for slot in 0..5 {
            let angle = slot as f32 * std::f32::consts::TAU / 5.0;
            let wanted = sim.unit(hall).unwrap().pos + Vec2::new(angle.cos(), angle.sin()) * 300.0;
            if let Some(pos) = free_site(&sim, wanted, 1)
                && let Some(worker) = sim.spawn_def_exact(owner as u8, worker_def, pos)
            {
                workers.push(worker);
            }
        }
        let mine = *mines
            .iter()
            .min_by(|a, b| {
                a.distance(*base)
                    .partial_cmp(&b.distance(*base))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("mine");
        if let Some(first) = workers.first().copied() {
            sim.step(&[wc3_sim::Command::Harvest {
                units: workers.clone(),
                to: mine,
            }]);
            if let Some(tree) = sim
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
            {
                let _ = sim.harvest_lumber(&workers[1..], tree);
            }
            if let Some(site) = free_site(&sim, *base + Vec2::new(420.0, 0.0), 2) {
                let _ = sim.place_building_ordered(first, supply_def, site);
            }
            if let Some(site) = free_site(&sim, *base + Vec2::new(-420.0, 160.0), 3) {
                let _ = sim.place_building_ordered(first, barracks_def, site);
            }
        }
        let mut config = wc3_sim::AiConfig::for_race(races[owner]);
        config.workers = 4;
        config.army_supply = 4;
        config.first_attack_tick = 300;
        config.wave_interval = 300;
        config.retreat_permille = 0;
        sim.enable_ai(owner as u8, config);
        sim.enable_squads(
            owner as u8,
            wc3_sim::SquadConfig {
                squad_size: 4,
                squad_size_bonus: 2,
                min_attack_units: 3,
                attack_force_interval: 45,
                rush_interval: 240,
                ..wc3_sim::SquadConfig::default()
            },
        );
    }

    let started = Instant::now();
    let mut last = Instant::now();
    let mut gold_deposited = [0i32; 2];
    let mut lumber_deposited = [0i32; 2];
    for tick in 0..ticks {
        sim.step(&[]);
        for event in &sim.events {
            match event {
                wc3_sim::Event::GoldDeposited { owner, amount } => {
                    gold_deposited[*owner as usize] += *amount;
                }
                wc3_sim::Event::LumberDeposited { owner, amount } => {
                    lumber_deposited[*owner as usize] += *amount;
                }
                _ => {}
            }
        }
        if tick % 600 == 0 {
            let alive0 = sim.units.iter().filter(|u| u.alive && u.owner == 0).count();
            let alive1 = sim.units.iter().filter(|u| u.alive && u.owner == 1).count();
            let bld0 = sim
                .units
                .iter()
                .filter(|u| u.alive && u.owner == 0 && u.is_building())
                .count();
            let bld1 = sim
                .units
                .iter()
                .filter(|u| u.alive && u.owner == 1 && u.is_building())
                .count();
            let trees = sim.destructables.iter().filter(|d| d.alive).count();
            let squads = sim
                .squads
                .iter()
                .map(|m| (m.owner, m.squads.len(), m.idle.len()))
                .collect::<Vec<_>>();
            let crew = |owner: u8, gold: bool| {
                sim.units
                    .iter()
                    .filter(|unit| {
                        unit.alive
                            && unit.owner == owner
                            && unit.def().stats.is_worker
                            && match unit.order {
                                wc3_sim::Order::Harvest { .. } => gold,
                                wc3_sim::Order::HarvestLumber { .. } => !gold,
                                _ => false,
                            }
                    })
                    .count()
            };
            println!(
                "t={tick:5} dt={:6.2}s alive={alive0}/{alive1} bld={bld0}/{bld1} trees={trees} squads={squads:?} hash={:016x}",
                last.elapsed().as_secs_f32(),
                sim.state_hash()
            );
            println!(
                "ECON t={tick:5} gold={}/{} lumber={}/{} crew_gold={}/{} crew_lumber={}/{}",
                sim.resources[0],
                sim.resources[1],
                sim.lumber[0],
                sim.lumber[1],
                crew(0, true),
                crew(1, true),
                crew(0, false),
                crew(1, false)
            );
            if tick % 3000 == 0 && tick > 0 {
                for owner in 0..2u8 {
                    let army: Vec<String> = sim
                        .units
                        .iter()
                        .filter(|unit| unit.alive && unit.owner == owner && !unit.is_building())
                        .map(|unit| {
                            format!(
                                "{}@{:.0},{:.0}:{:?}",
                                unit.def().id, unit.pos.x, unit.pos.y, unit.order
                            )
                        })
                        .collect();
                    println!("  army{owner}: {army:?}");
                }
            }
            last = Instant::now();
        }
        if sim.events.iter().any(|event| matches!(event, wc3_sim::Event::Victory(_))) {
            println!("victory at {tick} after {:.1}s", started.elapsed().as_secs_f32());
            break;
        }
    }
    println!("total {:.1}s", started.elapsed().as_secs_f32());
    let seconds = (sim.tick as f32 / wc3_sim::TICKS_PER_SECOND as f32).max(0.001);
    println!(
        "ECON_RATES sim_s={seconds:.1} delivered_gold={}/{} delivered_lumber={}/{} \
         gold_per_s={:.2}/{:.2} lumber_per_s={:.2}/{:.2} passive_income={}",
        gold_deposited[0],
        gold_deposited[1],
        lumber_deposited[0],
        lumber_deposited[1],
        gold_deposited[0] as f32 / seconds,
        gold_deposited[1] as f32 / seconds,
        lumber_deposited[0] as f32 / seconds,
        lumber_deposited[1] as f32 / seconds,
        sim.passive_income
    );
}
