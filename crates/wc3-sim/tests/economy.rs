//! Resource lifecycle, supply and upkeep tests.

mod common;

use common::*;
use std::path::Path;

use wc3_sim::{Command, Event, HarvestPhase, Order, Sim, UnitId, Vec2, data};

#[test]
fn harvest_round_trip_pays_ten_gold_and_depletes_the_mine() {
    let mut sim = Sim::new(41, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.add_gold_mine(Vec2::new(1400.0, 400.0), 10_000);
    let peasant = worker(&mut sim, 0, Vec2::new(600.0, 400.0));
    let before = sim.resources[0];
    sim.step(&[wc3_sim::Command::Harvest {
        units: vec![peasant],
        to: Vec2::new(1400.0, 400.0),
    }]);
    let mut saw_carry = false;
    let mut saw_hall = false;
    for _ in 0..1800 {
        sim.step(&[]);
        let unit = sim.unit(peasant).unwrap();
        if unit.carrying > 0 {
            saw_carry = true;
        }
        if matches!(
            unit.order,
            Order::Harvest {
                phase: wc3_sim::HarvestPhase::ToHall,
                ..
            }
        ) {
            saw_hall = true;
        }
    }
    assert!(saw_carry, "worker picks up a load");
    assert!(saw_hall, "worker returns to the hall");
    let gained = sim.resources[0] - before;
    eprintln!(
        "ECODBG gained={gained} mine={} carry={} order={:?}",
        sim.gold_mines[0].remaining,
        sim.unit(peasant).unwrap().carrying,
        sim.unit(peasant).unwrap().order
    );
    assert!(gained >= 40, "at least four trips at ten gold: {gained}");
    assert_eq!(gained % 10, 0, "each trip pays exactly ten");
    // A load in the worker's hands was already removed from the mine.
    let carried = sim.unit(peasant).unwrap().carrying;
    assert_eq!(sim.gold_mines[0].remaining, 10_000 - gained - carried);
}

#[test]
fn depleted_mine_releases_the_worker() {
    let mut sim = Sim::new(42, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.add_gold_mine(Vec2::new(1200.0, 400.0), 20);
    let peasant = worker(&mut sim, 0, Vec2::new(600.0, 400.0));
    sim.step(&[wc3_sim::Command::Harvest {
        units: vec![peasant],
        to: Vec2::new(1200.0, 400.0),
    }]);
    let events = step_collect(&mut sim, 1600);
    assert_eq!(sim.gold_mines[0].remaining, 0);
    assert!(has_error(&events, "gold mine depleted"));
    let unit = sim.unit(peasant).unwrap();
    assert!(
        !matches!(unit.order, Order::Harvest { .. }),
        "worker stops harvesting a dead mine"
    );
}

#[test]
fn only_workers_can_harvest() {
    let mut sim = Sim::new(43, 16, 16);
    let footman = footman(&mut sim, 0, Vec2::new(200.0, 200.0));
    sim.step(&[wc3_sim::Command::Harvest {
        units: vec![footman],
        to: Vec2::new(600.0, 600.0),
    }]);
    assert_eq!(sim.unit(footman).unwrap().order, Order::Idle);
}

#[test]
fn supply_cap_blocks_training_without_farms() {
    let mut sim = Sim::new(44, 32, 32);
    sim.passive_income = false;
    let hall_id = hall(&mut sim, 0, Vec2::new(500.0, 500.0));
    sim.step(&[]);
    assert_eq!(sim.supply_cap(0), 12, "town hall provides 12 food");
    sim.resources[0] = 100_000;
    let mut events = Vec::new();
    let mut footmen = 0;
    for _ in 0..12 {
        let before = sim
            .units_of(0)
            .filter(|unit| unit.def().id == "hfoo")
            .count();
        sim.step(&[wc3_sim::Command::Train {
            building: hall_id,
            kind: wc3_sim::UnitKind::Footman,
        }]);
        let batch = step_collect(&mut sim, 900);
        events.extend(batch);
        footmen = sim
            .units_of(0)
            .filter(|unit| unit.def().id == "hfoo")
            .count();
        if footmen == before && footmen >= 6 {
            break;
        }
    }
    assert_eq!(footmen, 6, "12 supply / 2 per footman");
    assert!(has_error(&events, "supply blocked"));
    assert_eq!(sim.supply_used(0), 12);
}

#[test]
fn farm_raises_the_supply_cap() {
    let mut sim = Sim::new(45, 32, 32);
    sim.passive_income = false;
    let hall_id = hall(&mut sim, 0, Vec2::new(500.0, 500.0));
    sim.step(&[]);
    assert_eq!(sim.supply_cap(0), 12);
    let farm = sim.spawn_def(0, "hhou", Vec2::new(900.0, 500.0)).unwrap();
    sim.step(&[]);
    assert_eq!(sim.unit(farm).unwrap().def().stats.supply_provided, 6);
    assert_eq!(sim.supply_cap(0), 18);
    let _ = hall_id;
}

#[test]
fn upkeep_taxes_gold_above_fifty_supply() {
    let mut sim = Sim::new(46, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    // Spawn directly so the supply cap does not refuse the setup.
    for i in 0..26 {
        footman(&mut sim, 0, Vec2::new(200.0 + (i % 5) as f32 * 40.0, 900.0));
    }
    sim.step(&[]);
    assert_eq!(sim.supply_used(0), 52);
    sim.add_gold_mine(Vec2::new(1200.0, 400.0), 1000);
    let peasant = worker(&mut sim, 0, Vec2::new(600.0, 400.0));
    sim.resources[0] = 0;
    sim.step(&[wc3_sim::Command::Harvest {
        units: vec![peasant],
        to: Vec2::new(1200.0, 400.0),
    }]);
    let events = step_collect(&mut sim, 900);
    let deposits = gold_events(&events);
    assert_eq!(
        deposits.first().map(|(_, amount)| *amount),
        Some(7),
        "70% upkeep"
    );
}

#[test]
fn no_upkeep_under_fifty_supply() {
    let mut sim = Sim::new(47, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.step(&[]);
    assert_eq!(sim.supply_used(0), 0);
    sim.add_gold_mine(Vec2::new(1200.0, 400.0), 1000);
    let peasant = worker(&mut sim, 0, Vec2::new(600.0, 400.0));
    sim.resources[0] = 0;
    sim.step(&[wc3_sim::Command::Harvest {
        units: vec![peasant],
        to: Vec2::new(1200.0, 400.0),
    }]);
    let events = step_collect(&mut sim, 900);
    assert_eq!(
        gold_events(&events).first().map(|(_, amount)| *amount),
        Some(10)
    );
}

#[test]
fn no_hall_harvest_falls_back_to_paying_at_the_mine() {
    let mut sim = Sim::new(48, 32, 32);
    sim.passive_income = false;
    sim.add_gold_mine(Vec2::new(900.0, 500.0), 500);
    let peasant = worker(&mut sim, 0, Vec2::new(500.0, 500.0));
    let before = sim.resources[0];
    sim.step(&[wc3_sim::Command::Harvest {
        units: vec![peasant],
        to: Vec2::new(900.0, 500.0),
    }]);
    step_n(&mut sim, 600);
    let mined = 500 - sim.gold_mines[0].remaining;
    assert!(mined >= 30, "legacy trickle mines: {mined}");
    assert_eq!(sim.resources[0] - before, mined, "gold equals ore mined");
}

#[test]
fn passive_income_is_opt_in() {
    // Melee has no stipend: the default sim must not pay one.
    let mut sim = Sim::new(49, 16, 16);
    assert!(!sim.passive_income, "passive income must be opt-in");
    let before = sim.resources[0];
    step_n(&mut sim, 300);
    assert_eq!(sim.resources[0], before);
    sim.passive_income = true;
    step_n(&mut sim, 300);
    assert!(sim.resources[0] > before, "the opt-in stipend still runs");
}

#[test]
fn visibility_and_explored_are_tracked_per_player() {
    let mut sim = Sim::new(50, 32, 32);
    let scout = sim.spawn_def(0, "hfoo", Vec2::new(1000.0, 1000.0)).unwrap();
    sim.step(&[]);
    assert!(sim.is_visible(0, Vec2::new(1000.0, 1000.0)));
    assert!(!sim.is_visible(1, Vec2::new(1000.0, 1000.0)));
    assert!(sim.is_explored(0, Vec2::new(1000.0, 1000.0)));
    let far = Vec2::new(3500.0, 3500.0);
    assert!(!sim.is_visible(0, far));
    assert!(!sim.is_explored(0, far));
    let _ = scout;
}


// ---------------------------------------------------------------------------
// Gather target interaction (blocked mine footprint, unreachable target,
// worker smart orders)
// ---------------------------------------------------------------------------

fn cell_world(x: u32, y: u32) -> Vec2 {
    Vec2::new(
        (x as f32 + 0.5) * wc3_sim::PATHING_CELL_SIZE,
        (y as f32 + 0.5) * wc3_sim::PATHING_CELL_SIZE,
    )
}

#[test]
fn economy_gather_targets_blocked_mine_footprint_is_reachable() {
    let mut sim = Sim::new(11, 64, 64);
    sim.passive_income = false;
    sim.resources = [1000, 1000];
    // A 4x4 blocked footprint in the middle: the nearest walkable cell centre
    // is 80 world units from the mine centre, more than the old 64-unit gate.
    let mut walkable = vec![true; 64 * 64];
    for y in 30..34u32 {
        for x in 30..34u32 {
            walkable[(y * 64 + x) as usize] = false;
        }
    }
    sim.set_pathing(64, 64, wc3_sim::PATHING_CELL_SIZE, walkable);
    let mine = cell_world(31, 31) + Vec2::new(wc3_sim::PATHING_CELL_SIZE * 0.5, wc3_sim::PATHING_CELL_SIZE * 0.5);
    sim.add_gold_mine(mine, 10_000);
    let hall = sim
        .spawn_def(0, "htow", cell_world(10, 10))
        .expect("hall");
    let peon = sim
        .spawn_def(0, "hpea", cell_world(20, 31))
        .expect("peasant");
    sim.step(&[]);
    let stop = sim.grid.cell_to_world(30, 31);
    eprintln!(
        "mine={mine:?} reach={:.1} gate={:.1} stop_cell={stop:?} dist={:.1}",
        sim.blocked_reach(mine),
        sim.gather_gate(mine, 56.0),
        stop.distance(mine)
    );
    assert!(
        sim.gather_gate(mine, 56.0) >= stop.distance(mine),
        "gate must accept the nearest walkable cell"
    );
    let before = sim.resources[0];
    sim.step(&[Command::Harvest {
        units: vec![peon],
        to: mine,
    }]);
    let mut gathering = false;
    for _ in 0..2400 {
        sim.step(&[]);
        if let Some(unit) = sim.unit(peon)
            && matches!(
                unit.order,
                Order::Harvest {
                    phase: HarvestPhase::Gathering | HarvestPhase::ToHall,
                    ..
                }
            )
        {
            gathering = true;
        }
    }
    eprintln!(
        "gold {before} -> {}, order={:?}",
        sim.resources[0],
        sim.unit(peon).map(|unit| unit.order.clone())
    );
    assert!(gathering, "worker must gather from the blocked mine");
    assert!(
        sim.resources[0] > before,
        "blocked-footprint mine must still pay gold"
    );
    let _ = (hall, stop);
}

#[test]
fn economy_gather_targets_unreachable_mine_reports_error() {
    let mut sim = Sim::new(12, 32, 32);
    sim.passive_income = false;
    sim.resources = [1000, 1000];
    let mut walkable = vec![true; 32 * 32];
    // Wall the mine off completely (ring of blocked cells with no opening).
    for y in 12..20u32 {
        for x in 12..20u32 {
            let ring = x == 12 || x == 19 || y == 12 || y == 19;
            if ring {
                walkable[(y * 32 + x) as usize] = false;
            }
        }
    }
    sim.set_pathing(32, 32, wc3_sim::PATHING_CELL_SIZE, walkable);
    let mine = cell_world(15, 15) + Vec2::new(64.0, 64.0);
    sim.add_gold_mine(mine, 10_000);
    let peon = sim
        .spawn_def(0, "hpea", cell_world(4, 4))
        .expect("peasant");
    sim.step(&[Command::Harvest {
        units: vec![peon],
        to: mine,
    }]);
    let mut errors = Vec::new();
    for event in &sim.events {
        if let Event::Error(message) = event {
            errors.push(message.to_string());
        }
    }
    for _ in 0..600 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::Error(message) = event {
                errors.push(message.to_string());
            }
        }
    }
    eprintln!("errors={errors:?} order={:?}", sim.unit(peon).map(|u| u.order.clone()));
    assert!(
        errors.iter().any(|message| message == "gather unreachable"),
        "unreachable gather must raise an explicit error, got {errors:?}"
    );
    assert!(
        matches!(sim.unit(peon).map(|unit| unit.order.clone()), Some(Order::Idle)),
        "unreachable gather leaves the worker idle, got {:?}",
        sim.unit(peon).map(|unit| unit.order.clone())
    );
}

#[test]
fn economy_gather_targets_worker_move_onto_mine_gathers() {
    let mut sim = Sim::new(13, 32, 32);
    sim.passive_income = false;
    sim.resources = [1000, 1000];
    let mine = cell_world(20, 16);
    sim.add_gold_mine(mine, 10_000);
    let hall = hall(&mut sim, 0, cell_world(6, 16));
    let peon = worker(&mut sim, 0, cell_world(10, 16));
    sim.step(&[]);
    // A right-click on the mine model resolves to a move onto the mine's
    // footprint: WC3 turns that into a gather order for a worker.
    sim.step(&[Command::Move {
        units: vec![peon],
        to: mine,
    }]);
    let order = sim.unit(peon).map(|unit| unit.order.clone());
    eprintln!("order after move-onto-mine = {order:?}");
    assert!(
        matches!(order, Some(Order::Harvest { .. })),
        "worker move onto a mine must become a gold gather, got {order:?}"
    );
    let before = sim.resources[0];
    for _ in 0..2400 {
        sim.step(&[]);
    }
    eprintln!("gold {before} -> {}", sim.resources[0]);
    assert!(sim.resources[0] > before, "smart gather must pay gold");
    let _ = hall;
}

#[test]
fn economy_gather_targets_worker_move_onto_tree_gathers() {
    let mut sim = Sim::new(14, 32, 32);
    sim.passive_income = false;
    sim.lumber = [1000, 1000];
    let tree = cell_world(20, 16);
    sim.spawn_destructable("LTlt", tree).expect("tree");
    let _hall = hall(&mut sim, 0, cell_world(6, 16));
    let peon = worker(&mut sim, 0, cell_world(10, 16));
    sim.step(&[Command::Move {
        units: vec![peon],
        to: tree,
    }]);
    let order = sim.unit(peon).map(|unit| unit.order.clone());
    eprintln!("order after move-onto-tree = {order:?}");
    assert!(
        matches!(order, Some(Order::HarvestLumber { .. })),
        "worker move onto a tree must become a lumber gather, got {order:?}"
    );
    let before = sim.lumber[0];
    for _ in 0..2400 {
        sim.step(&[]);
    }
    eprintln!("lumber {before} -> {}", sim.lumber[0]);
    assert!(sim.lumber[0] > before, "smart lumber must pay lumber");
}


// ---------------------------------------------------------------------------
// The shipped melee map `(2)Justice.w3x`: real mines and trees, real trips.
// The world is built exactly like `wc3_game::sim_bridge::spawn_map_scene`.
// ---------------------------------------------------------------------------

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn map_path() -> Option<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        eprintln!("skip: (2)Justice.w3x fixture missing at {}", path.display());
        return None;
    }
    Some(path)
}

fn build_world(bundle: &wc3_re::mapfiles::MapBundle) -> Sim {
    let origin = bundle
        .terrain
        .as_ref()
        .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
        .unwrap_or(Vec2::ZERO);
    let to_world = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let pathing = bundle.pathing.as_ref().expect("WPM pathing");
    let mut walkable = Vec::with_capacity(pathing.width as usize * pathing.height as usize);
    for y in 0..pathing.height {
        for x in 0..pathing.width {
            walkable.push(pathing.walkable(x, y));
        }
    }
    // Matches `Game::reset_scenario` + `spawn_map_scene`: WPM native cells,
    // real melee start resources, no passive stipend.
    let mut sim = Sim::new(0x3821, pathing.width, pathing.height);
    assert!(!sim.passive_income, "melee sim must not pay a passive drip");
    sim.resources = [1000, 1000];
    sim.lumber = [1000, 1000];
    sim.set_pathing(
        pathing.width,
        pathing.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );
    for unit in bundle.units.as_deref().unwrap_or(&[]) {
        let id = String::from_utf8_lossy(&unit.type_id).to_string();
        if id == "ngol" || id == "ugol" {
            sim.add_gold_mine(
                to_world(unit.position.x, unit.position.y),
                wc3_sim::GOLD_MINE_GOLD,
            );
            continue;
        }
        if id == "sloc" || data::def_of(&id).is_none() {
            continue;
        }
        let owner = if unit.player <= 1 { unit.player as u8 } else { 2 };
        sim.spawn_def_exact(owner, &id, to_world(unit.position.x, unit.position.y));
    }
    for doodad in bundle.doodads.as_deref().unwrap_or(&[]) {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        sim.spawn_destructable(&id, to_world(doodad.position.x, doodad.position.y));
    }
    sim
}

fn starts(bundle: &wc3_re::mapfiles::MapBundle) -> Vec<Vec2> {
    let origin = bundle
        .terrain
        .as_ref()
        .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
        .unwrap_or(Vec2::ZERO);
    let mut starts: Vec<(u32, Vec2)> = bundle
        .units
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|unit| &unit.type_id == b"sloc")
        .map(|unit| {
            (
                unit.player,
                Vec2::new(unit.position.x - origin.x, unit.position.y - origin.y),
            )
        })
        .collect();
    starts.sort_by_key(|(slot, _)| *slot);
    starts.into_iter().map(|(_, pos)| pos).collect()
}

/// Melee opening from `spawn_melee_bases`: an Orc great hall on a walkable
/// cell near the `sloc` marker plus five peons on the WC3 ring.
fn orc_opening(sim: &mut Sim, start: Vec2) -> Vec<UnitId> {
    let hall = sim
        .spawn_def(0, "ogre", sim.walkable_pos_near(start, 24).unwrap_or(start))
        .expect("orc hall");
    let hall_pos = sim.unit(hall).unwrap().pos;
    let mut workers = Vec::new();
    for slot in 0..5usize {
        let angle = std::f32::consts::TAU * slot as f32 / 5.0;
        let wanted = hall_pos + Vec2::new(angle.cos(), angle.sin()) * 300.0;
        let Some(pos) = sim.walkable_pos_near(wanted, 16) else {
            continue;
        };
        if let Some(peon) = sim.spawn_def(0, "opeo", pos) {
            workers.push(peon);
        }
    }
    workers
}

fn nearest_mine(sim: &Sim, pos: Vec2) -> Vec2 {
    sim.gold_mines
        .iter()
        .map(|mine| mine.pos)
        .min_by(|a, b| {
            a.distance(pos)
                .partial_cmp(&b.distance(pos))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("map gold mine")
}

fn nearest_tree(sim: &Sim, pos: Vec2) -> Vec2 {
    sim.destructables
        .iter()
        .filter(|tree| tree.alive)
        .map(|tree| tree.pos)
        .min_by(|a, b| {
            a.distance(pos)
                .partial_cmp(&b.distance(pos))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("map tree")
}

fn phase_text(order: Order) -> String {
    match order {
        Order::Harvest { phase, .. } => match phase {
            HarvestPhase::ToMine => "gold:ToMine".into(),
            HarvestPhase::Gathering => "gold:Gathering".into(),
            HarvestPhase::ToHall => "gold:ToHall".into(),
        },
        Order::HarvestLumber { phase, .. } => match phase {
            HarvestPhase::ToMine => "lumber:ToTree".into(),
            HarvestPhase::Gathering => "lumber:Gathering".into(),
            HarvestPhase::ToHall => "lumber:ToHall".into(),
        },
        other => format!("{other:?}"),
    }
}

#[test]
fn economy_worker_delivers_gold_and_lumber() {
    let Some(path) = map_path() else {
        return;
    };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("map bundle");
    // Slot 1 is the Orc player on (2)Justice; both seats must work alike.
    let starts = starts(&bundle);
    assert_eq!(starts.len(), 2, "(2)Justice start locations");
    for (seat, start) in starts.iter().copied().enumerate() {
        let mut sim = build_world(&bundle);
        let workers = orc_opening(&mut sim, start);
        assert_eq!(workers.len(), 5, "seat {seat}: five-worker melee opening");
        sim.step(&[]);
        let mine = nearest_mine(&sim, sim.unit(workers[0]).unwrap().pos);
        let tree = nearest_tree(&sim, sim.unit(workers[1]).unwrap().pos);
        let gold0 = sim.resources[0];
        let lumber0 = sim.lumber[0];
        sim.step(&[wc3_sim::Command::Harvest {
            units: vec![workers[0]],
            to: mine,
        }]);
        sim.step(&[wc3_sim::Command::HarvestLumber {
            units: vec![workers[1]],
            to: tree,
        }]);
        let mut gold_events: Vec<i32> = Vec::new();
        let mut lumber_events: Vec<i32> = Vec::new();
        let mut bounty_events: Vec<i32> = Vec::new();
        let mut saw_gather_gold = false;
        let mut saw_gather_lumber = false;
        let mut phase_gold = String::new();
        let mut phase_lumber = String::new();
        for tick in 0..1800u32 {
            sim.step(&[]);
            for event in &sim.events {
                match event {
                    Event::GoldDeposited { owner: 0, amount } => gold_events.push(*amount),
                    Event::LumberDeposited { owner: 0, amount } => lumber_events.push(*amount),
                    Event::Bounty { owner: 0, gold, .. } => bounty_events.push(*gold),
                    Event::Error(message) if *message == "gather unreachable" => {
                        panic!("seat {seat}: {message} at tick {tick}")
                    }
                    _ => {}
                }
            }
            let gold_order = sim.unit(workers[0]).unwrap().order;
            let lumber_order = sim.unit(workers[1]).unwrap().order;
            phase_gold = phase_text(gold_order);
            phase_lumber = phase_text(lumber_order);
            if matches!(
                gold_order,
                Order::Harvest {
                    phase: HarvestPhase::Gathering,
                    ..
                }
            ) {
                saw_gather_gold = true;
            }
            if matches!(
                lumber_order,
                Order::HarvestLumber {
                    phase: HarvestPhase::Gathering,
                    ..
                }
            ) {
                saw_gather_lumber = true;
            }
        }
        let gold_gain = sim.resources[0] - gold0;
        let lumber_gain = sim.lumber[0] - lumber0;
        eprintln!(
            "seat {seat} start={start:?} mine={mine:?} tree={tree:?}\n  \
             gold: {gold0}->{g0} (+{gold_gain}) deliveries={gold_events:?} phase={phase_gold}\n  \
             lumber: {lumber0}->{l1} (+{lumber_gain}) deliveries={lumber_events:?} phase={phase_lumber}",
            g0 = sim.resources[0],
            l1 = sim.lumber[0],
        );
        assert!(saw_gather_gold, "seat {seat}: worker never reached the mine");
        assert!(
            saw_gather_lumber,
            "seat {seat}: worker never reached the tree"
        );
        assert!(
            gold_events.len() >= 4,
            "seat {seat}: at least four gold trips in 60 s, got {:?}",
            gold_events
        );
        assert!(
            lumber_events.len() >= 4,
            "seat {seat}: at least four lumber trips in 60 s, got {:?}",
            lumber_events
        );
        assert!(
            gold_events.iter().all(|amount| *amount == 10),
            "seat {seat}: each gold trip pays exactly ten: {:?}",
            gold_events
        );
        assert!(
            lumber_events.iter().all(|amount| *amount == 10),
            "seat {seat}: each lumber trip pays exactly ten: {:?}",
            lumber_events
        );
        assert_eq!(
            gold_gain,
            gold_events.iter().sum::<i32>() + bounty_events.iter().sum::<i32>(),
            "seat {seat}: gold comes from deliveries and creep bounties only \
             (deliveries {gold_events:?}, bounties {bounty_events:?})"
        );
        assert_eq!(
            lumber_gain,
            lumber_events.iter().sum::<i32>(),
            "seat {seat}: lumber only comes from deliveries"
        );
        // 60 s of play: at least one delivery every 15 s per worker.
        assert!(
            gold_gain as f32 / 60.0 >= 0.6,
            "seat {seat}: gold rate {:.2}/s",
            gold_gain as f32 / 60.0
        );
        assert!(
            lumber_gain as f32 / 60.0 >= 0.6,
            "seat {seat}: lumber rate {:.2}/s",
            lumber_gain as f32 / 60.0
        );
    }
}

#[test]
fn economy_five_workers_saturate_mine_and_forest() {
    let Some(path) = map_path() else {
        return;
    };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("map bundle");
    let starts = starts(&bundle);
    let start = starts[1];
    let mut sim = build_world(&bundle);
    let workers = orc_opening(&mut sim, start);
    assert_eq!(workers.len(), 5);
    sim.step(&[]);
    let mine = nearest_mine(&sim, sim.unit(workers[0]).unwrap().pos);
    sim.step(&[wc3_sim::Command::Harvest {
        units: workers.clone(),
        to: mine,
    }]);
    let mut gold_ticks: Vec<u32> = Vec::new();
    let mut last = sim.resources[0];
    for tick in 0..1800u32 {
        sim.step(&[]);
        if sim.resources[0] > last {
            last = sim.resources[0];
            gold_ticks.push(tick);
        }
    }
    let gold_gain = sim.resources[0] - 1000;
    eprintln!(
        "5 workers gold: +{gold_gain} in 60 s ({:.2}/s), deliveries at {gold_ticks:?}",
        gold_gain as f32 / 60.0
    );
    assert!(
        gold_gain >= 100,
        "five workers must out-earn one by a wide margin: {gold_gain}"
    );

    let mut sim = build_world(&bundle);
    let workers = orc_opening(&mut sim, start);
    sim.step(&[]);
    for worker in &workers {
        let pos = sim.unit(*worker).unwrap().pos;
        let tree = nearest_tree(&sim, pos);
        sim.step(&[wc3_sim::Command::HarvestLumber {
            units: vec![*worker],
            to: tree,
        }]);
    }
    let lumber0 = sim.lumber[0];
    for _ in 0..1800 {
        sim.step(&[]);
    }
    let lumber_gain = sim.lumber[0] - lumber0;
    eprintln!(
        "5 workers lumber: +{lumber_gain} in 60 s ({:.2}/s)",
        lumber_gain as f32 / 60.0
    );
    assert!(
        lumber_gain >= 100,
        "five lumberjacks must deliver steadily: {lumber_gain}"
    );
}

#[test]
fn economy_idle_workers_do_not_create_gold() {
    let Some(path) = map_path() else {
        return;
    };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("map bundle");
    let mut sim = build_world(&bundle);
    let starts = starts(&bundle);
    let _workers = orc_opening(&mut sim, starts[1]);
    let gold0 = sim.resources[0];
    let lumber0 = sim.lumber[0];
    for _ in 0..300 {
        sim.step(&[]);
    }
    assert_eq!(
        sim.resources[0], gold0,
        "no worker delivery must mean no gold (passive drip off)"
    );
    assert_eq!(sim.lumber[0], lumber0, "no lumber without a trip");
}

/// Lumber parity: a worker whose tree dies keeps chopping the neighbouring
/// tree instead of dropping to `Idle` (measured on Justice: the first tree
/// lasts five loads and the loop used to stop there).
#[test]
fn economy_tree_retarget_keeps_lumber_loop() {
    let Some(path) = map_path() else {
        return;
    };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("map bundle");
    let mut sim = build_world(&bundle);
    let starts = starts(&bundle);
    let hall_pos = sim
        .spawn_def(0, "ogre", starts[1])
        .and_then(|hall| sim.unit(hall).map(|unit| unit.pos))
        .expect("hall");
    let wood = sim
        .spawn_def(0, "opeo", hall_pos + Vec2::new(-200.0, 120.0))
        .expect("peon");
    sim.step(&[]);
    let worker_pos = sim.unit(wood).unwrap().pos;
    let tree = nearest_tree(&sim, worker_pos);
    let tree_index = sim
        .destructables
        .iter()
        .position(|entry| entry.pos.distance(tree) <= 1.0 && entry.alive)
        .expect("tree entity");
    // A dead tree is one chop away: the loop must move on to a neighbour.
    sim.destructables[tree_index].hp = 10;
    let lumber0 = sim.lumber[0];
    sim.step(&[wc3_sim::Command::HarvestLumber {
        units: vec![wood],
        to: tree,
    }]);
    let mut retargeted = false;
    for _ in 0..2400 {
        sim.step(&[]);
        if let Some(unit) = sim.unit(wood)
            && let Order::HarvestLumber { tree: target, .. } = unit.order
            && target.distance(tree) > 1.0
        {
            retargeted = true;
        }
    }
    let gain = sim.lumber[0] - lumber0;
    eprintln!("tree retarget: +{gain} lumber, retargeted={retargeted}, order={:?}", sim.unit(wood).map(|u| u.order.clone()));
    assert!(retargeted, "the worker must retarget a neighbouring tree");
    assert!(
        gain >= 20,
        "lumber must keep flowing after the first tree dies: {gain}"
    );
}

/// `economy.pvp-bounty`: WC3 pays the killing player the victim's shipped gold
/// bounty for player-owned units as well as neutrals, exactly once per death.
/// Own-side kills and neutral killers pay nothing, and the neutral creep path
/// (`UnitBalance` `bountyplus + bountydice d bountysides`) is still single-pay.
#[test]
fn economy_pvp_bounty() {
    assert_eq!(
        data::unit_bounty("hfoo"),
        Some((20, 6, 3)),
        "footman bounty row from UnitBalance.slk: 20 + 6d3"
    );
    assert_eq!(
        data::unit_bounty("ogru"),
        Some((20, 6, 3)),
        "grunt bounty row: 20 + 6d3"
    );
    assert_eq!(
        data::unit_bounty("opeo"),
        Some((15, 5, 3)),
        "peon bounty row: 15 + 5d3"
    );

    // Player-vs-player: the killer is credited, the victim's purse is not.
    let mut sim = Sim::new(0xE01, 48, 48);
    assert!(!sim.passive_income, "the melee bounty path must not need a stipend");
    sim.resources = [500, 500];
    let killer = sim
        .spawn_def_exact(0, "hfoo", Vec2::new(800.0, 800.0))
        .expect("footman");
    let victim = sim
        .spawn_def_exact(1, "ogru", Vec2::new(860.0, 800.0))
        .expect("grunt");
    let victim_hp = sim.unit(victim).unwrap().hp;
    let (gold0_before, gold1_before) = (sim.resources[0], sim.resources[1]);
    let dealt = sim.apply_damage(killer, victim, victim_hp + 200, data::AttackType::Chaos);
    assert!(dealt > 0);
    assert!(!sim.unit(victim).unwrap().alive);
    let paid: Vec<(u8, i32)> = sim
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Bounty { owner, unit, gold } if *unit == victim => Some((*owner, *gold)),
            _ => None,
        })
        .collect();
    assert_eq!(paid.len(), 1, "exactly one bounty per enemy death: {paid:?}");
    assert_eq!(paid[0].0, 0, "paid to the killer");
    assert!(
        (26..=38).contains(&paid[0].1),
        "grunt 20 + 6d3 bounty: {}",
        paid[0].1
    );
    assert_eq!(
        sim.resources[0] - gold0_before,
        paid[0].1,
        "the killer's gold rises by exactly the bounty"
    );
    assert_eq!(
        sim.resources[1], gold1_before,
        "the victim's player pays nothing"
    );

    // A corpse cannot pay twice.
    sim.events.clear();
    let gold_after = sim.resources[0];
    assert_eq!(
        sim.apply_damage(killer, victim, 500, data::AttackType::Chaos),
        0,
        "a dead unit takes no damage"
    );
    assert_eq!(sim.resources[0], gold_after, "no second bounty");
    assert!(
        !sim.events
            .iter()
            .any(|event| matches!(event, Event::Bounty { .. })),
        "no second bounty event"
    );

    // Own-side kill: friendly fire pays nothing.
    let mut friendly = Sim::new(0xE02, 48, 48);
    friendly.resources = [500, 500];
    let ally_a = friendly
        .spawn_def_exact(0, "hfoo", Vec2::new(800.0, 800.0))
        .unwrap();
    let ally_b = friendly
        .spawn_def_exact(0, "hpea", Vec2::new(860.0, 800.0))
        .unwrap();
    let ally_hp = friendly.unit(ally_b).unwrap().hp;
    friendly.apply_damage(ally_a, ally_b, ally_hp + 100, data::AttackType::Chaos);
    assert!(!friendly.unit(ally_b).unwrap().alive);
    assert_eq!(friendly.resources[0], 500, "own-side kill pays nothing");
    assert!(
        !friendly
            .events
            .iter()
            .any(|event| matches!(event, Event::Bounty { .. })),
        "no bounty event for a friendly kill"
    );

    // Neutral killer: a creep killing a player unit pays that player nothing.
    let mut neutral_kill = Sim::new(0xE03, 48, 48);
    neutral_kill.resources = [500, 500];
    let creep = neutral_kill
        .spawn_def_exact(2, "nftr", Vec2::new(800.0, 800.0))
        .unwrap();
    let hunted = neutral_kill
        .spawn_def_exact(1, "ogru", Vec2::new(860.0, 800.0))
        .unwrap();
    let hunted_hp = neutral_kill.unit(hunted).unwrap().hp;
    neutral_kill.apply_damage(creep, hunted, hunted_hp + 100, data::AttackType::Chaos);
    assert!(!neutral_kill.unit(hunted).unwrap().alive);
    assert_eq!(
        neutral_kill.resources,
        [500, 500],
        "a neutral killer earns no player gold"
    );

    // Neutral creep kill still pays the player exactly once.
    let mut creeps = Sim::new(0xE04, 48, 48);
    creeps.resources = [0, 0];
    let hunter = creeps
        .spawn_def_exact(0, "hfoo", Vec2::new(800.0, 800.0))
        .unwrap();
    let creep = creeps
        .spawn_def_exact(2, "nftr", Vec2::new(860.0, 800.0))
        .unwrap();
    let creep_hp = creeps.unit(creep).unwrap().hp;
    creeps.apply_damage(hunter, creep, creep_hp + 100, data::AttackType::Chaos);
    let paid: Vec<i32> = creeps
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Bounty { owner: 0, unit, gold } if *unit == creep => Some(*gold),
            _ => None,
        })
        .collect();
    assert_eq!(paid.len(), 1, "one creep bounty: {paid:?}");
    assert!(
        (6..=8).contains(&paid[0]),
        "forest troll 5 + 1d3 bounty: {}",
        paid[0]
    );
    assert_eq!(creeps.resources[0], paid[0], "creep bounty credited once");
    creeps.events.clear();
    creeps.apply_damage(hunter, creep, 500, data::AttackType::Chaos);
    assert_eq!(creeps.resources[0], paid[0], "creep bounty never repeats");
}
