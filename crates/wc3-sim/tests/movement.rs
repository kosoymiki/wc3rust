//! Pathfinding, steering, formation and order-queue tests.

mod common;

use common::*;
use wc3_sim::{Order, OrderSpec, Sim, UnitKind, Vec2};

#[test]
fn unit_paths_around_a_wall_without_entering_blocked_cells() {
    let mut sim = Sim::new(21, 40, 40);
    // Stone wall from y=0..36 at x=20 with a gap at y>=36.
    for y in 0..36 {
        sim.grid.set_walkable(20, y, false);
    }
    let id = footman(&mut sim, 0, Vec2::new(128.0, 256.0));
    let goal = Vec2::new(38.0 * 128.0, 256.0);
    sim.step(&[wc3_sim::Command::Move {
        units: vec![id],
        to: goal,
    }]);
    let mut blocked_visits = 0;
    let mut arrived = false;
    for _ in 0..1200 {
        sim.step(&[]);
        let unit = sim.unit(id).unwrap();
        let cell = sim.grid.world_to_cell(unit.pos);
        if !sim.grid.walkable(cell.0, cell.1) {
            blocked_visits += 1;
        }
        if unit.order == Order::Idle && unit.pos.distance(goal) < 220.0 {
            arrived = true;
            break;
        }
    }
    assert_eq!(blocked_visits, 0, "unit never stands in a blocked cell");
    assert!(arrived, "unit routes through the wall gap and arrives");
}

#[test]
fn unit_escapes_a_concave_pocket() {
    let mut sim = Sim::new(22, 40, 40);
    // U-shaped pocket open to the north.
    for x in 10..18 {
        sim.grid.set_walkable(x, 20, false);
    }
    for y in 12..20 {
        sim.grid.set_walkable(10, y, false);
        sim.grid.set_walkable(17, y, false);
    }
    let id = footman(&mut sim, 0, Vec2::new(13.0 * 128.0, 16.0 * 128.0));
    let goal = Vec2::new(13.0 * 128.0, 3.0 * 128.0);
    sim.step(&[wc3_sim::Command::Move {
        units: vec![id],
        to: goal,
    }]);
    let mut arrived = false;
    for _ in 0..1500 {
        sim.step(&[]);
        let unit = sim.unit(id).unwrap();
        if unit.order == Order::Idle && unit.pos.distance(goal) < 220.0 {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "unit leaves the pocket instead of sticking");
}

#[test]
fn unreachable_goal_terminates_without_hanging() {
    let mut sim = Sim::new(23, 24, 24);
    // Box the unit in completely.
    for i in 4..12 {
        sim.grid.set_walkable(i, 4, false);
        sim.grid.set_walkable(i, 11, false);
        sim.grid.set_walkable(4, i, false);
        sim.grid.set_walkable(11, i, false);
    }
    let id = footman(&mut sim, 0, Vec2::new(8.0 * 128.0, 8.0 * 128.0));
    sim.step(&[wc3_sim::Command::Move {
        units: vec![id],
        to: Vec2::new(20.0 * 128.0, 20.0 * 128.0),
    }]);
    step_n(&mut sim, 900);
    let unit = sim.unit(id).unwrap();
    assert!(unit.alive);
    assert!(
        unit.order == Order::Idle || unit.stuck_attempts > 0,
        "bounded retries: order={:?} attempts={}",
        unit.order,
        unit.stuck_attempts
    );
}

#[test]
fn group_move_uses_formation_slots_and_does_not_stack() {
    let mut sim = Sim::new(24, 48, 48);
    let mut ids = Vec::new();
    for i in 0..24 {
        let pos = Vec2::new(256.0 + (i % 6) as f32 * 40.0, 256.0 + (i / 6) as f32 * 40.0);
        ids.push(footman(&mut sim, 0, pos));
    }
    let target = Vec2::new(2600.0, 2600.0);
    sim.step(&[wc3_sim::Command::Move {
        units: ids.clone(),
        to: target,
    }]);
    step_n(&mut sim, 900);
    let positions: Vec<Vec2> = ids.iter().map(|id| sim.unit(*id).unwrap().pos).collect();
    let mut arrived = 0;
    for pos in &positions {
        if pos.distance(target) < 420.0 {
            arrived += 1;
        }
    }
    assert!(arrived >= 20, "group arrived: {arrived}/24");
    let mut min_gap = f32::MAX;
    for i in 0..positions.len() {
        for j in i + 1..positions.len() {
            min_gap = min_gap.min(positions[i].distance(positions[j]));
        }
    }
    assert!(
        min_gap > 16.0,
        "units separate instead of stacking: {min_gap}"
    );
}

#[test]
fn separation_pushes_stacked_units_apart() {
    let mut sim = Sim::new(25, 24, 24);
    let mut ids = Vec::new();
    for _ in 0..12 {
        ids.push(footman(&mut sim, 0, Vec2::new(800.0, 800.0)));
    }
    sim.step(&[wc3_sim::Command::Move {
        units: ids.clone(),
        to: Vec2::new(1400.0, 800.0),
    }]);
    step_n(&mut sim, 120);
    let positions: Vec<Vec2> = ids.iter().map(|id| sim.unit(*id).unwrap().pos).collect();
    let mut spread: f32 = 0.0;
    for i in 0..positions.len() {
        for j in i + 1..positions.len() {
            spread = spread.max(positions[i].distance(positions[j]));
        }
    }
    assert!(spread > 40.0, "stacked spawns spread out: {spread}");
}

#[test]
fn patrol_traverses_both_endpoints() {
    let mut sim = Sim::new(26, 40, 40);
    let id = footman(&mut sim, 0, Vec2::new(600.0, 600.0));
    let a = Vec2::new(600.0, 600.0);
    let b = Vec2::new(1600.0, 600.0);
    sim.patrol(&[id], a, b);
    let mut touched_a = false;
    let mut touched_b = false;
    for _ in 0..1400 {
        sim.step(&[]);
        let pos = sim.unit(id).unwrap().pos;
        if pos.distance(a) < 90.0 {
            touched_a = true;
        }
        if pos.distance(b) < 90.0 {
            touched_b = true;
        }
        if touched_a && touched_b {
            break;
        }
    }
    assert!(
        touched_a && touched_b,
        "patrol visited a={touched_a} b={touched_b}"
    );
}

#[test]
fn queued_orders_execute_after_the_current_one() {
    let mut sim = Sim::new(27, 40, 40);
    let id = footman(&mut sim, 0, Vec2::new(400.0, 400.0));
    let first = Vec2::new(1200.0, 400.0);
    let second = Vec2::new(1200.0, 1600.0);
    sim.step(&[wc3_sim::Command::Move {
        units: vec![id],
        to: first,
    }]);
    sim.queue_order(&[id], OrderSpec::Move(second));
    assert_eq!(sim.unit(id).unwrap().order_queue.len(), 1);
    let mut reached_first = false;
    let mut reached_second = false;
    for _ in 0..1600 {
        sim.step(&[]);
        let unit = sim.unit(id).unwrap();
        let pos = unit.pos;
        if !reached_first && pos.distance(first) < 90.0 {
            reached_first = true;
        }
        if reached_first && pos.distance(second) < 90.0 {
            reached_second = true;
            break;
        }
    }
    assert!(reached_first, "first waypoint reached");
    assert!(reached_second, "queued second order executed");
}

#[test]
fn stop_clears_the_queue() {
    let mut sim = Sim::new(28, 32, 32);
    let id = footman(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.step(&[wc3_sim::Command::Move {
        units: vec![id],
        to: Vec2::new(1600.0, 400.0),
    }]);
    sim.queue_order(&[id], OrderSpec::Move(Vec2::new(1600.0, 1600.0)));
    sim.queue_order(&[id], OrderSpec::Hold);
    assert_eq!(sim.unit(id).unwrap().order_queue.len(), 2);
    sim.step(&[wc3_sim::Command::Stop { units: vec![id] }]);
    let unit = sim.unit(id).unwrap();
    assert_eq!(unit.order, Order::Idle);
    assert!(unit.order_queue.is_empty());
}

#[test]
fn hold_refuses_to_chase_but_attacks_in_range() {
    let mut sim = Sim::new(29, 32, 32);
    let archer = sim.spawn_def(0, "hrif", Vec2::new(500.0, 500.0)).unwrap();
    let enemy = footman(&mut sim, 1, Vec2::new(800.0, 500.0));
    sim.step(&[wc3_sim::Command::Hold {
        units: vec![archer],
    }]);
    let start = sim.unit(archer).unwrap().pos;
    step_n(&mut sim, 300);
    let archer_unit = sim.unit(archer).unwrap();
    assert_eq!(archer_unit.order, Order::Hold);
    assert!(archer_unit.pos.distance(start) < 8.0, "hold unit stays put");
    assert!(
        sim.unit(enemy).unwrap().hp < sim.unit(enemy).unwrap().max_hp,
        "hold unit still shoots the enemy"
    );
}

#[test]
fn move_into_blocked_goal_stops_at_nearest_walkable() {
    let mut sim = Sim::new(30, 32, 32);
    let id = footman(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.grid.set_walkable(20, 20, false);
    let target = Vec2::new(20.5 * 128.0, 20.5 * 128.0);
    sim.step(&[wc3_sim::Command::Move {
        units: vec![id],
        to: target,
    }]);
    step_n(&mut sim, 900);
    let unit = sim.unit(id).unwrap();
    assert!(unit.alive);
    let cell = sim.grid.world_to_cell(unit.pos);
    assert!(sim.grid.walkable(cell.0, cell.1));
    assert!(unit.pos.distance(target) < 300.0);
}

#[test]
fn building_footprint_blocks_pathing_and_frees_on_death() {
    let mut sim = Sim::new(31, 32, 32);
    let hall_id = sim
        .spawn_def(0, "htow", Vec2::new(8.0 * 128.0, 8.0 * 128.0))
        .unwrap();
    sim.step(&[]);
    let (cx, cy) = sim.grid.world_to_cell(Vec2::new(8.0 * 128.0, 8.0 * 128.0));
    assert!(!sim.grid.walkable(cx, cy), "hall footprint blocks its cell");
    let filler = footman(&mut sim, 0, Vec2::new(100.0, 100.0));
    sim.apply_damage(filler, hall_id, 999_999, wc3_sim::AttackType::Chaos);
    assert!(!sim.unit(hall_id).unwrap().alive);
    assert!(sim.grid.walkable(cx, cy), "footprint freed on death");
}

#[test]
fn many_units_do_not_stick_at_a_narrow_gate() {
    let mut sim = Sim::new(32, 40, 40);
    // Wall with a two-cell gate.
    for y in 0..40 {
        if !(18..20).contains(&y) {
            sim.grid.set_walkable(20, y, false);
        }
    }
    let mut ids = Vec::new();
    for i in 0..32 {
        let pos = Vec2::new(300.0 + (i % 8) as f32 * 60.0, 300.0 + (i / 8) as f32 * 60.0);
        ids.push(
            sim.spawn_def(0, if i % 2 == 0 { "hfoo" } else { "hrif" }, pos)
                .unwrap(),
        );
    }
    let goal = Vec2::new(34.0 * 128.0, 19.0 * 128.0);
    sim.step(&[wc3_sim::Command::Move {
        units: ids.clone(),
        to: goal,
    }]);
    step_n(&mut sim, 1400);
    let through = ids
        .iter()
        .filter(|id| {
            sim.unit(**id)
                .map(|u| u.pos.x > 21.0 * 128.0)
                .unwrap_or(false)
        })
        .count();
    assert!(
        through >= 24,
        "at least 24/32 units pass the gate, got {through}"
    );
}

#[test]
fn unit_kind_aliases_still_spawn_the_expected_roster_entry() {
    let mut sim = Sim::new(33, 16, 16);
    let id = sim.spawn(0, UnitKind::Archer, Vec2::new(300.0, 300.0));
    assert_eq!(sim.unit(id).unwrap().def().id, "hrif");
    assert_eq!(sim.unit(id).unwrap().kind, UnitKind::Archer);
}

#[test]
fn patrol_resumes_after_engaging_an_enemy() {
    let mut sim = Sim::new(35, 40, 40);
    let guard = footman(&mut sim, 0, Vec2::new(700.0, 700.0));
    let intruder = footman(&mut sim, 1, Vec2::new(1100.0, 700.0));
    let a = Vec2::new(700.0, 700.0);
    let b = Vec2::new(1900.0, 700.0);
    sim.patrol(&[guard], a, b);
    let mut fought = false;
    let mut touched_b = false;
    for _ in 0..2200 {
        sim.step(&[]);
        if sim.unit(intruder).map(|u| !u.alive).unwrap_or(false)
            || sim.unit(intruder).map(|u| u.hp < u.max_hp).unwrap_or(false)
        {
            fought = true;
        }
        if fought && sim.unit(guard).unwrap().pos.distance(b) < 120.0 {
            touched_b = true;
            break;
        }
    }
    assert!(fought, "patrol engaged the intruder");
    assert!(touched_b, "patrol resumed and reached the far endpoint");
    assert!(matches!(
        sim.unit(guard).unwrap().order,
        Order::Patrol { .. }
    ));
}
