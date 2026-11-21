//! mechanic `creeps.camps` — neutral camps, aggro leash, return home, neutral
//! hostility rules and the WC3 gold bounty, plus a real-map verification on
//! the shipped `(2)Justice.w3x`.
//!
//! The camp body is data driven: `Unit::is_camp_guard` accepts mobile armed
//! neutrals only, so shops, taverns, mercenary camps and critters never join a
//! camp and can never aggro.

mod common;

use std::path::Path;

use common::*;
use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::data::{self, def_of};
use wc3_sim::{
    CAMP_AGGRO_RADIUS, CAMP_CLUSTER_RADIUS, CAMP_HOME_RADIUS, CAMP_LEASH_RADIUS, Event, Order,
    Sim, Vec2,
};

fn guards_attacking(sim: &Sim, camp: usize) -> usize {
    sim.camps[camp]
        .units
        .iter()
        .filter_map(|unit| sim.unit(*unit))
        .filter(|unit| unit.alive && matches!(unit.order, Order::Attack(_)))
        .count()
}

fn camp_distance_max(sim: &Sim, camp: usize) -> f32 {
    let anchor = sim.camps[camp].anchor;
    sim.camps[camp]
        .units
        .iter()
        .filter_map(|unit| sim.unit(*unit))
        .filter(|unit| unit.alive)
        .map(|unit| unit.pos.distance(anchor))
        .fold(0.0f32, f32::max)
}

/// `creep.camp-aggro-leash`: guards sleep outside the camp radius, wake inside
/// it, and abandon a chase that leaves the leash.  WC3 profile: aggro 500,
/// leash 900.
#[test]
fn creep_camp_aggro_leash() {
    let mut sim = Sim::new(0xC01, 64, 64);
    let anchor = Vec2::new(2000.0, 2000.0);
    let camp = sim
        .spawn_camp(
            &["nftr", "nban"],
            anchor,
            CAMP_AGGRO_RADIUS,
            CAMP_LEASH_RADIUS,
        )
        .expect("camp");
    assert_eq!(camp, 1);
    assert_eq!(sim.camps[0].aggro, 500.0, "WC3 camp aggro radius");
    assert_eq!(sim.camps[0].leash, 900.0, "WC3 MaxGuardDistance leash");

    // Outside the aggro ring (and outside every acquire range) the camp sleeps.
    let far = footman(&mut sim, 0, anchor + Vec2::new(780.0, 0.0));
    step_n(&mut sim, 90);
    assert_eq!(guards_attacking(&sim, 0), 0, "no aggro outside the camp");
    assert!(
        !matches!(sim.unit(far).unwrap().order, Order::Attack(_)),
        "the visitor does not pick a fight on his own"
    );

    // Step onto the camp edge: the guards wake and pick the visitor.
    let edge = anchor + Vec2::new(CAMP_AGGRO_RADIUS - 40.0, 0.0);
    sim.teleport_unit(far, edge);
    let mut woke = false;
    for _ in 0..90 {
        sim.step(&[]);
        if guards_attacking(&sim, 0) > 0 {
            woke = true;
            break;
        }
    }
    assert!(woke, "camp aggro inside {CAMP_AGGRO_RADIUS}");
    let chasing = sim.camps[0]
        .units
        .iter()
        .filter_map(|unit| sim.unit(*unit))
        .filter(|unit| matches!(unit.order, Order::Attack(_)))
        .count();
    assert!(chasing >= 1, "at least one guard chases");

    // Run outside the leash: the chase is dropped and the camp walks home.
    sim.teleport_unit(far, anchor + Vec2::new(1500.0, 0.0));
    sim.step(&[wc3_sim::Command::Stop { units: vec![far] }]);
    for _ in 0..600 {
        sim.step(&[]);
    }
    assert_eq!(guards_attacking(&sim, 0), 0, "the leash breaks the chase");
    assert!(
        camp_distance_max(&sim, 0) <= CAMP_HOME_RADIUS + 60.0,
        "guards walked home: max distance {}",
        camp_distance_max(&sim, 0)
    );
}

/// `creep.return-home`: a damaaged guard that is dragged out of the leash
/// restores to full HP once the camp is calm again, and does not heal while an
/// intruder is still inside the aggro ring.
#[test]
fn creep_return_home() {
    let mut sim = Sim::new(0xC02, 64, 64);
    let anchor = Vec2::new(2000.0, 2000.0);
    sim.spawn_camp(
        &["nftr", "nban"],
        anchor,
        CAMP_AGGRO_RADIUS,
        CAMP_LEASH_RADIUS,
    )
    .expect("camp");
    let intruder = footman(&mut sim, 0, anchor + Vec2::new(420.0, 0.0));
    step_n(&mut sim, 60);
    let guard = sim.camps[0]
        .units
        .iter()
        .copied()
        .find(|unit| sim.unit(*unit).map(|unit| unit.alive).unwrap_or(false))
        .expect("a living guard");
    let max_hp = sim.unit(guard).unwrap().max_hp;
    let dealt = sim.apply_damage(intruder, guard, 40, data::AttackType::Normal);
    assert!(dealt > 0, "the player can hit a neutral");
    let wounded = sim.unit(guard).unwrap().hp;
    assert!(wounded < max_hp, "guard is wounded: {wounded}/{max_hp}");
    assert!(
        matches!(sim.unit(guard).unwrap().order, Order::Attack(_)),
        "an attacked camp retaliates"
    );

    // The intruder stays: no healing while the camp is hot.
    step_n(&mut sim, 120);
    assert!(
        sim.unit(guard).unwrap().hp <= wounded,
        "no return-home heal while the intruder stands inside aggro ({} > {wounded})",
        sim.unit(guard).unwrap().hp
    );

    // The intruder leaves: the guard walks home and is restored.
    sim.teleport_unit(intruder, Vec2::new(6000.0, 6000.0));
    sim.step(&[wc3_sim::Command::Stop {
        units: vec![intruder],
    }]);
    for _ in 0..600 {
        sim.step(&[]);
    }
    let guard_unit = sim.unit(guard).unwrap();
    assert!(!matches!(guard_unit.order, Order::Attack(_)));
    assert!(
        guard_unit.pos.distance(anchor) <= CAMP_HOME_RADIUS + 60.0,
        "guard is home: {}",
        guard_unit.pos.distance(anchor)
    );
    assert_eq!(
        guard_unit.hp, max_hp,
        "camp heals a guard that returned home"
    );
    assert_eq!(guard_unit.mana, guard_unit.max_mana);
}

/// `creep.shop-neutral`: trade buildings (`ngme`, `ngad`, `nmr8`, `ntav`) are
/// never camp members and never aggro, even with a player unit on top of them.
#[test]
fn creep_shop_never_aggros() {
    let mut sim = Sim::new(0xC03, 32, 32);
    let shop = sim
        .spawn_def(2, "ngme", Vec2::new(1500.0, 1500.0))
        .expect("goblin merchant");
    assert!(sim.unit(shop).unwrap().is_trade_building());
    assert!(
        sim.register_camp(Vec2::new(1500.0, 1500.0), &[shop], 500.0, 900.0)
            .is_none(),
        "a shop cannot form a camp"
    );
    let visitor = footman(&mut sim, 0, Vec2::new(1500.0, 1620.0));
    assert_eq!(sim.cluster_neutral_camps(), 0, "no creeps, no camps");
    let shop_hp = sim.unit(shop).unwrap().hp;
    step_n(&mut sim, 90);
    assert_eq!(
        sim.unit(shop).map(|unit| unit.order),
        Some(Order::Idle),
        "the merchant never attacks"
    );
    let _ = shop_hp;
    assert!(
        sim.unit(visitor).unwrap().alive,
        "the visitor survives standing next to the merchant"
    );
    // A shop next to a real camp is still not a member of that camp.
    sim.spawn_def(2, "nftr", Vec2::new(1560.0, 1560.0));
    sim.cluster_neutral_camps();
    assert!(
        sim.camps.iter().all(|camp| !camp.units.contains(&shop)),
        "shops are excluded from camp bodies"
    );
    assert_eq!(sim.camps.len(), 1, "the armed neutral still guards a camp");
}

/// `creep.critters`: passive neutral animals (sheep, dog, crab) do not fight,
/// do not join camps and run from approaching player units.
#[test]
fn creep_critters_flee() {
    let mut sim = Sim::new(0xC04, 32, 32);
    let sheep = sim
        .spawn_def(2, "nshe", Vec2::new(1500.0, 1500.0))
        .expect("sheep");
    assert!(sim.unit(sheep).unwrap().is_critter());
    assert!(
        !sim.unit(sheep).unwrap().is_camp_guard(),
        "critters never guard a camp"
    );
    assert_eq!(sim.cluster_neutral_camps(), 0, "no camp from a lone critter");

    let threat = Vec2::new(1500.0, 1800.0);
    let _walker = footman(&mut sim, 0, threat);
    let sheep_start = sim.unit(sheep).unwrap().pos;
    let start = sheep_start.distance(threat);
    step_n(&mut sim, 45);
    let sheep_now = sim.unit(sheep).unwrap().pos;
    let end = sheep_now.distance(threat);
    assert!(
        end > start,
        "the sheep fled away from the threat: {start} -> {end} ({sheep_start:?} -> {sheep_now:?})"
    );
    let fleeing = sheep_now - sheep_start;
    assert!(
        fleeing.dot(threat - sheep_start) < 0.0,
        "flight points away from the player unit: {fleeing:?}"
    );
    assert!(
        matches!(sim.unit(sheep).unwrap().order, Order::Move(_)),
        "flight is a move order"
    );
}

/// `creep.player-attacks-neutral`: a player unit can order an attack on a
/// neutral, damage lands, and the camp fights back only inside its leash.
#[test]
fn creep_player_attacks_neutral() {
    let mut sim = Sim::new(0xC05, 64, 64);
    let anchor = Vec2::new(2000.0, 2000.0);
    sim.spawn_camp(&["nban"], anchor, CAMP_AGGRO_RADIUS, CAMP_LEASH_RADIUS)
        .expect("camp");
    let guard = sim.camps[0].units[0];
    let attacker = footman(&mut sim, 0, anchor + Vec2::new(300.0, 0.0));
    let before = sim.unit(guard).unwrap().hp;
    sim.step(&[wc3_sim::Command::Attack {
        units: vec![attacker],
        target: guard,
    }]);
    for _ in 0..90 {
        sim.step(&[]);
    }
    let after = sim.unit(guard).unwrap().hp;
    assert!(after < before, "the neutral took damage: {before} -> {after}");
    assert!(
        matches!(sim.unit(guard).unwrap().order, Order::Attack(_)),
        "the neutral retaliates"
    );
    // Chase cap: the player runs away and the guard stops chasing.
    sim.teleport_unit(attacker, anchor + Vec2::new(1600.0, 0.0));
    sim.step(&[wc3_sim::Command::Stop { units: vec![attacker] }]);
    for _ in 0..600 {
        sim.step(&[]);
    }
    assert_eq!(guards_attacking(&sim, 0), 0, "no chase across the map");
}

// ---------------------------------------------------------------------------
// Real map: (2)Justice.w3x
// ---------------------------------------------------------------------------

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

/// Creep camps formed by the map's data-backed neutral placements (measured,
/// see `examples/creep_census.rs`): 88 neutral placements > 76 armed creeps in
/// 22 proximity camps, 10 neutral structures and 2 critters.
const JUSTICE_CAMPS: usize = 22;
const JUSTICE_CREEP_GUARDS: usize = 76;

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

/// Boots the map the way `wc3_game::sim_bridge` does: WPM pathing, then every
/// data-backed neutral placement at [`wc3_sim::NEUTRAL_OWNER`], then creep-camp
/// clustering.
fn justice_sim(map: &Justice) -> Sim {
    let origin = Vec2::new(map.terrain.center.0, map.terrain.center.1);
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
    for unit in &map.units {
        let def_id = String::from_utf8_lossy(&unit.type_id).to_string();
        if def_id == "sloc" || def_id == "ngol" || def_id == "ugol" {
            continue;
        }
        if def_of(&def_id).is_none() {
            continue;
        }
        let pos = Vec2::new(unit.position.x - origin.x, unit.position.y - origin.y);
        sim.spawn_def_exact(wc3_sim::NEUTRAL_OWNER, &def_id, pos);
    }
    sim.cluster_neutral_camps();
    sim
}

/// `creeps.justice-camps`: on the shipped 1v1 map the camps are real: the
/// measured camp/guard counts, a visitor at the camp edge wakes it, leaving the
/// leash resets it, a kill pays the WC3 bounty and hero XP exactly once, and
/// walking across the map moves cells through all three fog states.
#[test]
fn creeps_justice_camps_aggro_leash_bounty_and_fog() {
    let Some(map) = load_justice() else {
        return;
    };
    let mut sim = justice_sim(&map);

    let guards: usize = sim
        .camps
        .iter()
        .map(|camp| {
            camp.units
                .iter()
                .filter(|unit| sim.unit(**unit).map(|unit| unit.alive).unwrap_or(false))
                .count()
        })
        .sum();
    eprintln!(
        "JUSTICE camps={} guards={} aggro={CAMP_AGGRO_RADIUS} leash={CAMP_LEASH_RADIUS} cluster={CAMP_CLUSTER_RADIUS}",
        sim.camps.len(),
        guards
    );
    assert_eq!(sim.camps.len(), JUSTICE_CAMPS, "creep camps on Justice");
    assert_eq!(guards, JUSTICE_CREEP_GUARDS, "armed neutral guards");
    assert!(
        sim.camps
            .iter()
            .flat_map(|camp| camp.units.iter())
            .all(|unit| sim.unit(*unit).map(|unit| unit.is_camp_guard()).unwrap_or(false)),
        "every camp member is an armed mobile neutral"
    );
    assert!(
        sim.camps
            .iter()
            .all(|camp| camp.aggro == CAMP_AGGRO_RADIUS && camp.leash == CAMP_LEASH_RADIUS),
        "map camps use the WC3 aggro/leash profile"
    );

    // Stage a player unit at the camp edge.
    let camp_index = 0;
    let anchor = sim.camps[camp_index].anchor;
    let edge = sim
        .walkable_pos_near(anchor + Vec2::new(CAMP_AGGRO_RADIUS - 60.0, 0.0), 8)
        .expect("walkable ground next to the camp");
    assert!(
        edge.distance(anchor) < CAMP_AGGRO_RADIUS,
        "the staged unit is inside the aggro ring: {}",
        edge.distance(anchor)
    );
    let visitor = sim.spawn_def(0, "hfoo", edge).expect("footman");
    let mut woke = false;
    for _ in 0..120 {
        sim.step(&[]);
        if guards_attacking(&sim, camp_index) > 0 {
            woke = true;
            break;
        }
    }
    assert!(woke, "the Justice camp aggroed on approach");

    // Leave the leash: the camp resets (guards stop chasing and go home).
    let refuge = Vec2::new(600.0, 600.0);
    sim.teleport_unit(visitor, refuge);
    sim.step(&[wc3_sim::Command::Stop {
        units: vec![visitor],
    }]);
    for _ in 0..900 {
        sim.step(&[]);
    }
    assert_eq!(
        guards_attacking(&sim, camp_index),
        0,
        "the camp dropped the chase after the visitor left the leash"
    );
    assert!(
        camp_distance_max(&sim, camp_index) <= CAMP_LEASH_RADIUS,
        "guards stay inside their leash: {}",
        camp_distance_max(&sim, camp_index)
    );

    // Kill the camp with a hero standing next to it: bounty + XP once.
    let hero = sim
        .spawn_def(0, "Hpal", anchor + Vec2::new(220.0, 0.0))
        .expect("paladin");
    let gold_before = sim.resources[0];
    let xp_before = sim.unit(hero).unwrap().xp;
    let camp_units = sim.camps[camp_index].units.clone();
    let living: usize = camp_units
        .iter()
        .filter(|unit| sim.unit(**unit).map(|unit| unit.alive).unwrap_or(false))
        .count();
    assert!(living > 0);
    let mut bounty_events = 0;
    let mut bounty_gold = 0;
    for unit in &camp_units {
        let Some(target) = sim.unit(*unit) else {
            continue;
        };
        if !target.alive {
            continue;
        }
        let event_start = sim.events.len();
        let dealt = sim.apply_damage(hero, *unit, target.max_hp * 4, data::AttackType::Chaos);
        assert!(dealt > 0, "the hero kills a camp guard");
        bounty_events += sim.events[event_start..]
            .iter()
            .filter(|event| matches!(event, Event::Bounty { .. }))
            .count();
        bounty_gold += sim.events[event_start..]
            .iter()
            .filter_map(|event| match event {
                Event::Bounty { gold, .. } => Some(*gold),
                _ => None,
            })
            .sum::<i32>();
    }
    assert_eq!(
        bounty_events, living,
        "one bounty event per guard, exactly once"
    );
    assert!(bounty_gold > 0, "creeps pay WC3 gold bounty");
    assert_eq!(
        sim.resources[0] - gold_before,
        bounty_gold,
        "the bounty is credited to the killer"
    );
    let xp_after = sim.unit(hero).unwrap().xp;
    assert!(xp_after > xp_before, "kills grant hero XP: {xp_before} -> {xp_after}");

    // Re-damaging a corpse does nothing: no double bounty, no double XP.
    let corpse = camp_units[0];
    let gold_after_kill = sim.resources[0];
    let xp_after_kill = sim.unit(hero).unwrap().xp;
    let dealt = sim.apply_damage(hero, corpse, 9999, data::AttackType::Chaos);
    assert_eq!(dealt, 0, "a dead unit takes no damage");
    assert_eq!(sim.resources[0], gold_after_kill, "no second bounty");
    assert_eq!(sim.unit(hero).unwrap().xp, xp_after_kill, "no second XP");

    // Fog transitions across a walk on the real map.
    let start = sim
        .walkable_pos_near(Vec2::new(1800.0, 1800.0), 16)
        .expect("walkable start");
    let walker = sim.spawn_def(0, "hfoo", start).expect("walker");
    sim.step(&[]);
    let start_cell = sim.grid.world_to_cell(start);
    assert_eq!(
        sim.visibility(0, start_cell),
        wc3_sim::Visibility::Visible,
        "the walker's start cell is visible"
    );
    let goal = sim
        .walkable_pos_near(start + Vec2::new(2200.0, 0.0), 24)
        .expect("walkable goal");
    for _ in 0..700 {
        sim.step(&[wc3_sim::Command::Move {
            units: vec![walker],
            to: goal,
        }]);
    }
    let walked = sim.unit(walker).unwrap().pos;
    assert!(
        walked.distance(start) > 400.0,
        "the unit walked on the real map: {:?} -> {:?}",
        start,
        walked
    );
    assert_eq!(
        sim.visibility(0, start_cell),
        wc3_sim::Visibility::Explored,
        "the start cell dropped to explored"
    );
    assert_eq!(
        sim.visibility(0, sim.grid.world_to_cell(walked)),
        wc3_sim::Visibility::Visible,
        "the walked-to cell is visible"
    );
    let unexplored = Vec2::new(15000.0, 4000.0);
    assert_eq!(
        sim.visibility(0, sim.grid.world_to_cell(unexplored)),
        wc3_sim::Visibility::Unexplored,
        "unvisited corners stay unexplored"
    );
}
