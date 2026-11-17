//! mechanic `fog.visibility` — per-player three-state fog of war.
//!
//! The sim owns the mask; the renderer only samples it.  These tests pin the
//! three states (`Unexplored | Explored | Visible`), the sight-radius source
//! (`UnitBalance.slk` `sight`, carried on every unit/building), the monotonic
//! explored grid, the audio gate and the last-known enemy blips.

mod common;

use common::*;
use wc3_sim::data::def_of;
use wc3_sim::{Command, Sim, Vec2, Visibility};

fn cell_of(sim: &Sim, pos: Vec2) -> (u32, u32) {
    sim.grid.world_to_cell(pos)
}

/// `vision.three-state-fog`: a cell starts unexplored, becomes visible while a
/// friendly unit is in range, and drops to explored (remembered) once the unit
/// walks away.  The explored grid never goes back to unexplored.
#[test]
fn vision_three_state_fog() {
    let mut sim = Sim::new(0xF06, 64, 64);
    let start = Vec2::new(1000.0, 1000.0);
    let scout = footman(&mut sim, 0, start);
    sim.step(&[]);

    let start_cell = cell_of(&sim, start);
    assert_eq!(
        sim.visibility(0, start_cell),
        Visibility::Visible,
        "the unit's own cell is visible to its owner"
    );
    assert_eq!(
        sim.visibility(1, start_cell),
        Visibility::Unexplored,
        "player 1 has no units and sees nothing"
    );
    let far_cell = cell_of(&sim, Vec2::new(6000.0, 6000.0));
    assert_eq!(sim.visibility(0, far_cell), Visibility::Unexplored);
    assert!(!sim.is_explored(0, Vec2::new(6000.0, 6000.0)));

    // Walk north-east far enough that the start cell leaves the sight radius.
    let goal = start + Vec2::new(2400.0, 0.0);
    for _ in 0..420 {
        sim.step(&[Command::Move {
            units: vec![scout],
            to: goal,
        }]);
    }
    let pos = sim.unit(scout).unwrap().pos;
    assert!(pos.x > start.x + 1600.0, "the scout actually walked: {pos:?}");

    assert_eq!(
        sim.visibility(0, start_cell),
        Visibility::Explored,
        "the old cell is remembered but no longer visible"
    );
    assert!(sim.is_explored(0, start));
    assert!(!sim.is_visible(0, start));
    let now_cell = cell_of(&sim, pos);
    assert_eq!(
        sim.visibility(0, now_cell),
        Visibility::Visible,
        "the walked-to cell is visible"
    );
    assert_eq!(sim.visibility_at(0, pos), Visibility::Visible);
    assert!(sim.is_visible_at(pos));
    assert_eq!(Visibility::from_state(0), Visibility::Unexplored);
    assert_eq!(Visibility::from_state(1), Visibility::Explored);
    assert_eq!(Visibility::from_state(2), Visibility::Visible);
    assert_eq!(Visibility::Visible.as_u8(), 2);
}

/// `vision.sight-radius`: the revealed disc is the unit's `sight` from the
/// generated table; a living building reveals for as long as it stands and its
/// death drops the mask back to explored.
#[test]
fn vision_sight_radius() {
    let mut sim = Sim::new(0xF07, 64, 64);
    let anchor = Vec2::new(2600.0, 2600.0);
    let scout = footman(&mut sim, 0, anchor);
    let sight = sim.unit(scout).unwrap().sight;
    assert_eq!(
        sight,
        def_of("hfoo").unwrap().stats.sight,
        "unit sight comes from the def"
    );
    assert!(sight > 0.0);
    sim.step(&[]);
    assert!(sim.is_visible(0, anchor + Vec2::new(sight * 0.9, 0.0)));
    assert!(sim.is_visible(0, anchor + Vec2::new(0.0, -sight * 0.9)));
    let outside = anchor + Vec2::new(sight * 1.35, 0.0);
    assert!(!sim.is_visible(0, outside), "outside the sight disc");
    assert_eq!(sim.visibility_at(0, outside), Visibility::Unexplored);

    // A building reveals its own sight radius while alive.
    let hall_pos = Vec2::new(6000.0, 6000.0);
    let hall = hall(&mut sim, 0, hall_pos);
    let hall_sight = sim.unit(hall).unwrap().sight;
    assert_eq!(
        hall_sight,
        def_of("htow").unwrap().stats.sight,
        "buildings reveal from the same table column"
    );
    assert!(hall_sight > 0.0);
    sim.step(&[]);
    let hall_probe = hall_pos + Vec2::new(hall_sight * 0.8, 0.0);
    assert!(sim.is_visible(0, hall_probe), "a building grants vision");

    // Kill the building: the same cell is remembered, not visible.
    let enemy = sim.spawn_def(1, "ugho", Vec2::new(7600.0, 6000.0)).unwrap();
    let hp = sim.unit(hall).unwrap().hp;
    let dealt = sim.apply_damage(
        enemy,
        hall,
        hp * 2,
        wc3_sim::data::AttackType::Normal,
    );
    assert!(dealt > 0, "the hall takes damage");
    assert!(!sim.unit(hall).unwrap().alive, "the hall died");
    sim.step(&[]);
    assert!(!sim.is_visible(0, hall_probe), "no vision from a dead hall");
    assert!(sim.is_explored(0, hall_probe), "explored memory survives");
    assert_eq!(
        sim.explored_grid(0).len(),
        64 * 64,
        "one explored byte per cell"
    );
    assert!(sim.explored_fraction(0) > 0.0);
    let mask = sim.fog_mask(0);
    assert_eq!(mask.len(), 64 * 64);
    assert!(mask.contains(&Visibility::Explored.as_u8()));
    assert!(mask.contains(&Visibility::Visible.as_u8()));
}

/// `vision.audibility-gate`: `is_audible` accepts visible sources and nearby
/// sources inside the audible radius, and rejects distant unseen ones.
#[test]
fn vision_audibility_gate() {
    let mut sim = Sim::new(0xF08, 64, 64);
    let player = footman(&mut sim, 0, Vec2::new(2000.0, 2000.0));
    let enemy = sim.spawn_def(1, "ugho", Vec2::new(6000.0, 2000.0)).unwrap();
    sim.step(&[]);
    let enemy_pos = sim.unit(enemy).unwrap().pos;
    assert!(!sim.is_visible(0, enemy_pos), "the enemy is in fog");
    assert!(
        !sim.is_audible(0, enemy_pos, 500.0),
        "4000 units away and unseen: silent"
    );
    assert!(
        sim.is_audible(0, enemy_pos, 4500.0),
        "inside the audible radius: heard"
    );
    let own_pos = sim.unit(player).unwrap().pos;
    assert!(sim.is_audible(0, own_pos, 1.0), "own cell is visible");
    assert!(sim.is_audible(1, Vec2::new(6000.0, 2000.0), 1.0));
}

/// `vision.last-seen`: enemy units leave a last-known blip in the remembered
/// fog even after they walk out of sight.
#[test]
fn vision_last_seen_blips() {
    let mut sim = Sim::new(0xF09, 64, 64);
    let scout = footman(&mut sim, 0, Vec2::new(2000.0, 2000.0));
    let hidden = footman(&mut sim, 0, Vec2::new(500.0, 500.0));
    let enemy = sim.spawn_def(1, "ugho", Vec2::new(2600.0, 2000.0)).unwrap();
    sim.step(&[]);
    let seen = sim.last_seen(0, enemy).expect("enemy seen while visible");
    assert!(sim.is_visible(0, seen));
    assert_eq!(sim.last_seen_entry(0, enemy).unwrap().tick, 0);
    assert!(
        sim.last_seen(1, scout).is_some(),
        "sight is mutual: the grunt sees the scout too"
    );
    assert!(
        sim.last_seen(1, hidden).is_none(),
        "player 1 never saw the far-away footman"
    );

    // The enemy leaves the sight radius: the blip keeps the old position.
    let hideout = Vec2::new(7400.0, 7400.0);
    sim.teleport_unit(enemy, hideout);
    sim.teleport_unit(scout, Vec2::new(5600.0, 2000.0));
    sim.step(&[]);
    assert!(!sim.is_visible(0, hideout));
    let blip = sim
        .last_seen(0, enemy)
        .expect("the blip survives fog");
    assert_eq!(blip, seen, "last-known position, not the hidden one");
    assert_eq!(
        sim.visibility_at(0, blip),
        Visibility::Explored,
        "the blip sits on remembered terrain"
    );
    let all = sim.last_seen_all(0);
    assert_eq!(all.len(), 1, "one enemy blip tracked");
    assert_eq!(all[0].0, enemy);
}
