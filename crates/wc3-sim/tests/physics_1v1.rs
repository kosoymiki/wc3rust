//! 1v1 physics: native WPM pathing, collision/turn rate and the real WC3
//! movement constants.

mod common;

use common::*;
use wc3_formats::wpm::PathingMap;
use wc3_sim::{Command, Event, PATHING_CELL_SIZE, Race, Sim, Vec2, def_of, rotate_towards};

fn human04_wpm() -> Option<PathingMap> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
    if !path.is_file() {
        return None;
    }
    let base = wc3_formats::map::map_mpq_base(&path).ok()?;
    let archive = wc3_mpq::MpqArchive::open_at(&path, base).ok()?;
    let bytes = archive.read("war3map.wpm").ok()?;
    PathingMap::parse(&bytes).ok()
}

fn walkable_vec(map: &PathingMap) -> Vec<bool> {
    let mut walkable = Vec::with_capacity(map.width as usize * map.height as usize);
    for y in 0..map.height {
        for x in 0..map.width {
            walkable.push(map.walkable(x, y));
        }
    }
    walkable
}

#[test]
fn native_wpm_grid_paths_units_without_stuck() {
    let Some(map) = human04_wpm() else {
        eprintln!("skip: Human04 wpm missing");
        return;
    };
    assert_eq!((map.width, map.height), (512, 512), "native WPM resolution");
    let walkable = walkable_vec(&map);
    assert!(walkable.iter().filter(|cell| **cell).count() > 10_000);
    let mut sim = Sim::new(7, 128, 128);
    sim.set_pathing(map.width, map.height, PATHING_CELL_SIZE, walkable.clone());
    assert_eq!(sim.grid.cell_size, 32.0);
    assert_eq!((sim.grid.width, sim.grid.height), (512, 512));
    // A 3x3 walkable patch with 20 walkable cells straight ahead.
    let mut start = None;
    'outer: for y in 4..map.height - 8 {
        for x in 4..map.width - 24 {
            let patch_ok = (-1i32..=1).all(|dy| {
                (-1i32..=1).all(|dx| {
                    let yy = (y as i32 + dy) as u32;
                    let xx = (x as i32 + dx) as u32;
                    walkable[(yy * map.width + xx) as usize]
                })
            });
            let line_ok = (0..20).all(|dx| walkable[(y * map.width + x + dx) as usize]);
            if patch_ok && line_ok {
                start = Some((x, y));
                break 'outer;
            }
        }
    }
    let (sx, sy) = start.expect("walkable corridor");
    let start_pos = sim.grid.cell_to_world(sx, sy);
    let goal_pos = sim.grid.cell_to_world(sx + 18, sy);
    let unit = sim.spawn_def(0, "hfoo", start_pos).expect("footman");
    sim.step(&[]);
    sim.step(&[Command::Move {
        units: vec![unit],
        to: goal_pos,
    }]);
    let mut arrived = false;
    let mut blocked = false;
    for _ in 0..900 {
        sim.step(&[]);
        if sim
            .events
            .iter()
            .any(|event| matches!(event, Event::Error("path blocked")))
        {
            blocked = true;
            break;
        }
        if sim.unit(unit).map(|unit| unit.pos.distance(goal_pos)) < Some(48.0) {
            arrived = true;
            break;
        }
    }
    assert!(
        arrived && !blocked,
        "arrived={arrived} blocked={blocked} pos={:?} goal={goal_pos:?}",
        sim.unit(unit).map(|unit| unit.pos)
    );
}

#[test]
fn turn_rate_and_physics_constants_come_from_the_tables() {
    let footman = def_of("hfoo").expect("footman").stats;
    assert_eq!(footman.speed, 270.0, "UnitBalance.spd");
    assert_eq!(footman.turn_rate, 0.6, "UnitData.turnRate");
    assert_eq!(footman.acquire_range, 500.0, "UnitWeapons.acquire");
    assert_eq!(footman.radius, 15.5, "UnitData.collision/2");
    assert_eq!(footman.armor, 2, "UnitBalance.def");
    assert_eq!(footman.damage, 11, "UnitWeapons.dmgplus1");
    assert_eq!(footman.attack_ticks, 40, "cooldown * 30");
    let grunt = def_of("ogru").expect("grunt").stats;
    assert_eq!(grunt.speed, 270.0);
    assert_eq!(grunt.turn_rate, 0.6);
    assert_eq!(grunt.damage, 17);
    // Rotation is capped per tick (0.6 turn/s at 30 Hz = 0.1257 rad/tick).
    assert!((rotate_towards(0.0, std::f32::consts::PI, 0.1) - 0.1).abs() < 1e-6);
    let mut sim = Sim::new(11, 64, 64);
    let unit = sim
        .spawn_def(0, "hfoo", Vec2::new(700.0, 400.0))
        .expect("unit");
    sim.step(&[]);
    sim.step(&[Command::Move {
        units: vec![unit],
        to: Vec2::new(300.0, 400.0),
    }]);
    sim.step(&[]);
    let facing = sim.unit(unit).expect("unit").facing.abs();
    assert!(
        facing > 0.0 && facing < 0.5,
        "facing turns gradually, got {facing}"
    );
}

#[test]
fn collision_separates_units_that_share_a_goal() {
    let mut sim = Sim::new(12, 64, 64);
    let a = sim
        .spawn_def(0, "hfoo", Vec2::new(600.0, 600.0))
        .expect("a");
    let b = sim
        .spawn_def(0, "hfoo", Vec2::new(620.0, 600.0))
        .expect("b");
    sim.step(&[]);
    sim.step(&[Command::Move {
        units: vec![a, b],
        to: Vec2::new(900.0, 600.0),
    }]);
    for _ in 0..120 {
        sim.step(&[]);
    }
    let pa = sim.unit(a).expect("a").pos;
    let pb = sim.unit(b).expect("b").pos;
    let radii = sim.unit(a).expect("a").radius() + sim.unit(b).expect("b").radius();
    assert!(
        pa.distance(pb) >= radii - 1.0,
        "units overlap: {:?} vs {:?}",
        pa,
        pb
    );
}

#[test]
fn melee_1v1_setup_spawns_halls_mines_and_five_workers() {
    let mut sim = Sim::new(42, 128, 128);
    sim.passive_income = false;
    let setup = sim
        .setup_melee_1v1(
            Race::Orc,
            Race::Human,
            Vec2::new(900.0, 900.0),
            Vec2::new(3000.0, 3000.0),
        )
        .expect("melee 1v1");
    assert_eq!(setup.player_workers, 5);
    assert_eq!(setup.enemy_workers, 5);
    assert_eq!(sim.unit(setup.player_hall).unwrap().def().id, "ogre");
    assert_eq!(sim.unit(setup.player_hall).unwrap().owner, 0);
    assert_eq!(sim.unit(setup.enemy_hall).unwrap().def().id, "htow");
    assert_eq!(sim.unit(setup.enemy_hall).unwrap().owner, 1);
    // 1 starting mine per seat + 2 expansion mines per side.
    assert_eq!(sim.gold_mines.len(), 2 + 4);
    assert_eq!(sim.camps.len(), 6);
    assert_eq!(
        sim.units_of(0)
            .filter(|unit| unit.alive && unit.def().id == "opeo")
            .count(),
        5,
        "orc peons"
    );
    assert_eq!(
        sim.units_of(1)
            .filter(|unit| unit.alive && unit.def().id == "hpea")
            .count(),
        5,
        "human peasants"
    );
    assert!(sim.ai.iter().any(|ai| ai.owner == 1), "human AI enabled");
    for unit in sim
        .units
        .iter()
        .filter(|unit| unit.alive && !unit.is_building())
    {
        let cell = sim.grid.world_to_cell(unit.pos);
        assert!(
            sim.grid.walkable(cell.0, cell.1),
            "{} spawned on a blocked cell",
            unit.def().id
        );
    }
    // Hall footprints intentionally block their cells.
    for hall in [setup.player_hall, setup.enemy_hall] {
        let unit = sim.unit(hall).expect("hall");
        assert!(!unit.blocked_cells.is_empty());
        assert!(
            unit.blocked_cells
                .iter()
                .all(|&(x, y)| !sim.grid.walkable(x, y))
        );
    }
}
