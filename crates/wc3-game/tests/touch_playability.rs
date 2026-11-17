//! Device-parity touch economy: the pointer events the phone sends must select
//! an Orc peon and turn a tap on a tree / a mine into a real harvest order.
//!
//! Mechanic ids proven here (MECHANICS_GAP_MAP.md):
//!   * `economy.lumber` — tap tree -> HarvestLumber -> lumber deposited
//!   * `economy.gold`   — tap mine -> Harvest -> gold deposited
//!   * `ui.selection`   — precondition: the tap selects the drawn model, and
//!                        the command card follows that selection
//!
//! POLICY: a failure must name the exact cause, never just "lumber did not
//! rise". Every assertion prints, on failure:
//!   * the TAP log line (screen point, world point, `picked=` unit)
//!   * the worker's post-tap order (gold vs lumber vs move vs idle)
//!   * the time budget and the sampled trip trace (phase/pos/path/carry)
//!   * every `Event::Error` the sim raised during the trip (e.g. "no tree").
//! The sim mechanism itself is proven by `wc3-sim/tests/economy.rs`; this file
//! proves the *touch -> order* wiring on the real Justice map with the real
//! MPQ install.
use std::path::{Path, PathBuf};

use glam::Vec3;
use wc3_game::assets::Assets;
use wc3_game::game::Game;
use wc3_sim::{Event, HarvestPhase, Order, UnitId, Vec2};

/// Hard cap on simulated ticks even when the distance-derived budget is wrong.
const MAX_TICKS: u32 = 9_000;

fn install() -> Option<(PathBuf, PathBuf)> {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())?;
    let map = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x");
    if !map.is_file() {
        eprintln!("skip: install/map missing (mpq={mpq:?} map={map:?})");
        return None;
    }
    Some((mpq, map))
}

fn new_game(mpq: &Path, map: &Path) -> Game {
    let mut game = Game::new(Assets::load(Some(mpq), Some(map)), 0x5EED);
    // The device runs landscape 2414x1080 (analysis/agent-qa device runs).
    game.resize(2414.0, 1080.0);
    game
}

/// A finger cannot produce two taps in the same millisecond: let the clock run
/// so the next tap is a single tap (the double-tap window is ~300 ms). Without
/// this, the tree/mine tap inherits `double=true` from the selection tap and
/// the failure would blame the wrong code path.
fn settle(game: &mut Game) {
    for _ in 0..15 {
        game.update(1.0 / 30.0);
    }
}

/// Screen point a finger lands on for a world position at `lift` above ground.
fn screen_of(game: &Game, world: Vec2, lift: f32) -> Vec2 {
    let ground = game.assets.height_at(world.x, world.y);
    let screen = game
        .camera
        .world_to_screen(Vec3::new(world.x, ground + lift, world.y))
        .unwrap_or_else(|| {
            panic!(
                "world ({:.0},{:.0}) +{lift} is behind the camera (viewport {:?})",
                world.x, world.y, game.viewport
            )
        });
    // A finger can only land inside the window. An off-viewport projection
    // means the camera was not centred on the target, and the tap would be
    // delivered to a completely different world point.
    assert!(
        screen.x >= 0.0 && screen.y >= 0.0 && screen.x <= game.viewport.x && screen.y <= game.viewport.y,
        "world ({:.0},{:.0}) projects to screen ({:.0},{:.0}), outside the {:.0}x{:.0} viewport — \
         centre the camera on the target before tapping",
        world.x, world.y, screen.x, screen.y, game.viewport.x, game.viewport.y
    );
    screen
}

fn tap(game: &mut Game, screen: Vec2) {
    game.pointer_down(screen);
    game.pointer_up(screen);
}

fn last_log<'a>(game: &'a Game, prefix: &str) -> Option<&'a str> {
    game.log
        .iter()
        .rev()
        .find(|line| line.starts_with(prefix))
        .map(String::as_str)
}

fn order_text(order: Order) -> String {
    match order {
        Order::Idle => "Idle".to_string(),
        Order::Move(p) => format!("Move({:.0},{:.0})", p.x, p.y),
        Order::Attack(id) => format!("Attack(#{})", id.0),
        Order::MoveAttack(p) => format!("MoveAttack({:.0},{:.0})", p.x, p.y),
        Order::Hold => "Hold".to_string(),
        Order::Patrol { a, b, .. } => format!("Patrol({:.0},{:.0}<->{:.0},{:.0})", a.x, a.y, b.x, b.y),
        Order::Harvest { mine, phase } => format!(
            "Harvest GOLD mine=({:.0},{:.0}) phase={}",
            mine.x,
            mine.y,
            phase_text(phase)
        ),
        Order::HarvestLumber { tree, phase } => format!(
            "HarvestLumber tree=({:.0},{:.0}) phase={}",
            tree.x,
            tree.y,
            phase_text(phase)
        ),
        Order::Build(id) => format!("Build(#{})", id.0),
        Order::Repair(id) => format!("Repair(#{})", id.0),
    }
}

fn phase_text(phase: HarvestPhase) -> &'static str {
    match phase {
        HarvestPhase::ToMine => "ToTarget",
        HarvestPhase::Gathering => "Gathering",
        HarvestPhase::ToHall => "ToHall",
    }
}

struct Worker {
    id: UnitId,
    pos: Vec2,
    def_id: &'static str,
    model_height: f32,
}

fn starting_worker(game: &Game) -> Worker {
    let player = game.player;
    game.sim
        .alive_units()
        .find(|unit| unit.owner == player && unit.def().stats.is_worker)
        .map(|unit| Worker {
            id: unit.id,
            pos: unit.pos,
            def_id: unit.def().id,
            model_height: game.assets.model_for_def(unit.def().id).height,
        })
        .expect("the melee start must spawn a worker")
}

/// Tap the worker's own model and prove the tap actually selected it and the
/// card followed. Returns the screen point used.
fn select_worker(game: &mut Game, worker: &Worker) -> Vec2 {
    let screen = screen_of(game, worker.pos, worker.model_height * 0.55);
    tap(game, screen);
    let tap_log = last_log(game, "TAP ").unwrap_or("<no TAP log>").to_string();
    assert!(
        game.selection.contains(&worker.id),
        "ui.selection: tap at screen ({:.0},{:.0}) on the worker model did not select it.\n  \
         worker=#{} def={} pos=({:.0},{:.0}) model_h={:.0}\n  selection={:?}\n  {tap_log}",
        screen.x,
        screen.y,
        worker.id.0,
        worker.def_id,
        worker.pos.x,
        worker.pos.y,
        worker.model_height,
        game.selection
    );
    let builds: Vec<&str> = game.menu.iter().map(|entry| entry.id.as_str()).collect();
    // The operator's map may seat the local player in either slot (w3i), so the
    // expected build list comes from the sim for `game.player`, never from a
    // hard-coded race.
    let expected: Vec<&str> = game
        .sim
        .build_menu_for_owner(game.player)
        .iter()
        .map(|def| def.id)
        .collect();
    assert!(
        builds.len() >= 5,
        "ui.selection: the worker card must list the race's whole build list on the same tap, got {builds:?}\n  {tap_log}"
    );
    assert!(
        expected.iter().any(|id| builds.contains(id)),
        "ui.selection: the card does not follow the tap — expected ids from build_menu_for_owner({}) {expected:?}, got {builds:?}\n  {tap_log}",
        game.player
    );
    screen
}

struct Sample {
    tick: u64,
    phase: String,
    pos: Vec2,
    path: usize,
    carrying: i32,
    timer: u32,
}
impl std::fmt::Display for Sample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "t{:5} {:<34} pos=({:7.1},{:7.1}) path={:<3} carry={:<3} timer={:<3}",
            self.tick, self.phase, self.pos.x, self.pos.y, self.path, self.carrying, self.timer
        )
    }
}

/// One simulated tick plus the trace sample and any sim error it raised.
fn advance(game: &mut Game, worker: UnitId, tick: u64, trace: &mut Vec<Sample>, errors: &mut Vec<String>) {
    game.update(1.0 / 30.0);
    for event in &game.sim.events {
        if let Event::Error(message) = event {
            let line = format!("t{tick}: {message}");
            if !errors.contains(&line) {
                errors.push(line);
            }
        }
    }
    if let Some(unit) = game.sim.unit(worker) {
        let phase = order_text(unit.order);
        let sample = Sample {
            tick,
            phase,
            pos: unit.pos,
            path: unit.path.len(),
            carrying: unit.carrying.max(unit.carrying_lumber),
            timer: unit.harvest_timer,
        };
        // 1 Hz trace + every phase change, so a failure shows the whole trip.
        if tick % 30 == 0 || trace.last().map_or(true, |prev| prev.phase != sample.phase) {
            trace.push(sample);
        }
    }
}

fn trace_text(trace: &[Sample], tail: usize) -> String {
    let start = trace.len().saturating_sub(tail);
    trace[start..]
        .iter()
        .map(|sample| format!("    {sample}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Time budget from the actual walking distance (2 legs + gather + slack).
/// A fixed 600-tick constant was the earlier gate and it hid a legitimate
/// long trip as a failure; the budget is derived, never guessed.
fn trip_budget(game: &Game, worker: &Worker, target: Vec2) -> u32 {
    let speed = game
        .sim
        .unit(worker.id)
        .map(|unit| unit.speed.max(1.0))
        .unwrap_or(1.0);
    let hall = game
        .sim
        .alive_units()
        .filter(|unit| {
            unit.owner == game.player && unit.def().stats.supply_provided > 0 && unit.construct.is_none()
        })
        .map(|unit| unit.pos.distance(target))
        .fold(f32::INFINITY, f32::min);
    let legs = worker.pos.distance(target) + if hall.is_finite() { hall } else { 0.0 };
    let ticks = ((legs * 2.0 / speed) * 30.0) as u32 + 600;
    ticks.clamp(900, MAX_TICKS)
}

#[test]
fn economy_lumber_touch_tap_orders_harvest() {
    let Some((mpq, map)) = install() else {
        return;
    };
    let mut game = new_game(&mpq, &map);
    let worker = starting_worker(&game);
    // The initial camera sits on the base, not on the worker: a tap can only
    // land on the model when the model is inside the viewport.
    game.camera.centre_on(worker.pos);
    select_worker(&mut game, &worker);
    settle(&mut game);
    let player = game.player as usize;

    // Nearest tree to the peon: exactly what a player taps first.
    let (tree, tree_distance) = game
        .sim
        .destructables
        .iter()
        .filter(|tree| tree.alive)
        .map(|tree| (tree.pos, tree.pos.distance(worker.pos)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("the real melee map must spawn trees");
    let nearest_mine = game.sim.nearest_gold_mine(worker.pos);
    game.camera.centre_on(tree);
    let mut screen = screen_of(&game, tree, 20.0);
    // The camera looks at ground height 0 while this map's terrain sits well
    // above it, so a centred tree can project *under the top telemetry strip*.
    // A finger can only reach the playfield rectangle between the strip and the
    // console; put the tree there instead of tapping a HUD band. The camera eye
    // sits south of the target, so aiming south of the tree moves it down.
    let playfield_top = game
        .mobile_layout
        .as_ref()
        .map(|layout| layout.top_meta.bottom())
        .unwrap_or(0.0);
    let playfield_bottom = game
        .mobile_layout
        .as_ref()
        .map(|layout| layout.console.y)
        .unwrap_or(game.viewport.y);
    let mut aim = tree;
    for _ in 0..16 {
        if screen.y > playfield_top + 40.0 && screen.y < playfield_bottom - 40.0 {
            break;
        }
        aim -= Vec2::new(0.0, 150.0);
        game.camera.centre_on(aim);
        screen = screen_of(&game, tree, 20.0);
    }
    assert!(
        screen.y > playfield_top + 40.0 && screen.y < playfield_bottom - 40.0,
        "the tree must project inside the playfield band ({playfield_top}..{playfield_bottom}): \
         screen={screen:?} viewport={:?}",
        game.viewport
    );
    let lumber0 = game.sim.lumber[player];
    let gold0 = game.sim.resources[player];
    tap(&mut game, screen);
    let tap_log = last_log(&game, "TAP ").unwrap_or("<no TAP log>").to_string();
    game.update(1.0 / 30.0);

    let (order, pos_after) = game
        .sim
        .unit(worker.id)
        .map(|unit| (unit.order, unit.pos))
        .expect("worker alive");
    let order_desc = order_text(order);
    eprintln!(
        "TAP tree: world=({:.0},{:.0}) dist={:.0} screen=({:.0},{:.0}) mine={:?}\n  \
         {tap_log}\n  order after tap: {order_desc}\n  worker moved {:.1}u",
        tree.x,
        tree.y,
        tree_distance,
        screen.x,
        screen.y,
        nearest_mine,
        pos_after.distance(worker.pos)
    );

    match order {
        Order::HarvestLumber { tree: target, .. } => assert!(
            target.distance(tree) <= 1.0,
            "economy.lumber: the harvest target ({:.0},{:.0}) is not the tapped tree ({:.0},{:.0}).\n  {tap_log}",
            target.x,
            target.y,
            tree.x,
            tree.y
        ),
        Order::Harvest { mine, .. } => panic!(
            "economy.lumber: INVENTORY ROUTING — the tap on a tree at world ({:.0},{:.0}) ordered a GOLD \
             harvest of the mine at ({:.0},{:.0}), not lumber.\n  \
             `touch_tap_intent` checks `sim.gold_mines` BEFORE `sim.destructables` (game.rs:2602-2624); \
             a tree whose screen distance to a mine is < 72*layout.scale is stolen by the mine branch.\n  \
             {tap_log}\n  order: {order_desc}",
            tree.x, tree.y, mine.x, mine.y
        ),
        Order::Idle | Order::Move(_) | Order::MoveAttack(_) => panic!(
            "economy.lumber: TOUCH INTENT NOT REACHED — a tap on the tree ({:.0},{:.0}) produced `{order_desc}`.\n  \
             Either the tap was consumed by a picked unit (see `picked=` above; `pick_unit` wins before \
             `touch_tap_intent`), or `near_screen` reject the tree (it projects at ground y=0, radius \
             72*layout.scale={:.0}px).\n  {tap_log}\n  nearest mine={:?} (mine branch wins first)",
            tree.x,
            tree.y,
            72.0 * game.layout.scale,
            nearest_mine
        ),
        other => panic!(
            "economy.lumber: tap on the tree produced an unexpected order `{}`.\n  {tap_log}",
            order_text(other)
        ),
    }

    let budget = trip_budget(&game, &worker, tree);
    eprintln!("economy.lumber: 10 lumber/trip, budget {budget} ticks");
    let mut trace = Vec::new();
    let mut errors = Vec::new();
    let start_tick = game.sim.tick;
    let mut picked_tick = None;
    let mut deposited_tick = None;
    let mut tick = 0u32;
    while tick < budget {
        tick += 1;
        let tick_now = game.sim.tick - start_tick;
        advance(&mut game, worker.id, tick_now, &mut trace, &mut errors);
        let unit = game.sim.unit(worker.id).expect("worker alive");
        if picked_tick.is_none() && unit.carrying_lumber > 0 {
            picked_tick = Some(game.sim.tick - start_tick);
        }
        if game.sim.lumber[player] > lumber0 {
            deposited_tick = Some(game.sim.tick - start_tick);
            break;
        }
        if unit.hp <= 0 {
            break;
        }
    }
    let lumber1 = game.sim.lumber[player];
    let gold1 = game.sim.resources[player];
    let unit = game.sim.unit(worker.id).expect("worker alive");
    eprintln!(
        "PARITY economy.lumber touch_tap: lumber {lumber0}->{lumber1} gold {gold0}->{gold1} \
         picked_at={picked_tick:?} deposited_at={deposited_tick:?} tick={} order={}",
        game.sim.tick - start_tick,
        order_text(unit.order)
    );
    if lumber1 <= lumber0 {
        let trip = if picked_tick.is_some() {
            "the worker cut a load but never deposited it: inspect ToHall pathing/deposit radius (210u)"
        } else if unit.pos.distance(worker.pos) < 8.0 {
            "the worker never moved: the HarvestLumber movement leg is not running (order_movement/ToTarget)"
        } else {
            "the worker moved but never reached Gathering: inspect the 72u tree stop radius"
        };
        panic!(
            "economy.lumber: tapping a tree did not pay lumber ({lumber0}->{lumber1}, gold {gold0}->{gold1}).\n  \
             CAUSE CLASS: {trip}\n  \
             state: order={} pos=({:.0},{:.0}) path={} carry={} timer={}\n  \
             sim errors: {:?}\n  last trace:\n{}\n  {tap_log}",
            order_text(unit.order),
            unit.pos.x,
            unit.pos.y,
            unit.path.len(),
            unit.carrying_lumber,
            unit.harvest_timer,
            errors,
            trace_text(&trace, 14)
        );
    }
    assert!(
        picked_tick.is_some(),
        "economy.lumber: lumber rose but no carry was observed — deposit came from another source"
    );
}

#[test]
fn economy_gold_touch_tap_orders_harvest() {
    let Some((mpq, map)) = install() else {
        return;
    };
    let mut game = new_game(&mpq, &map);
    let worker = starting_worker(&game);
    // The initial camera sits on the base, not on the worker: a tap can only
    // land on the model when the model is inside the viewport.
    game.camera.centre_on(worker.pos);
    select_worker(&mut game, &worker);
    settle(&mut game);
    let player = game.player as usize;

    let mine = game
        .sim
        .nearest_gold_mine(worker.pos)
        .expect("the real melee map must spawn gold mines");
    game.camera.centre_on(mine);
    let screen = screen_of(&game, mine, 20.0);
    let gold0 = game.sim.resources[player];
    let lumber0 = game.sim.lumber[player];
    tap(&mut game, screen);
    let tap_log = last_log(&game, "TAP ").unwrap_or("<no TAP log>").to_string();
    game.update(1.0 / 30.0);

    let (order, pos_after) = game
        .sim
        .unit(worker.id)
        .map(|unit| (unit.order, unit.pos))
        .expect("worker alive");
    let order_desc = order_text(order);
    eprintln!(
        "TAP mine: world=({:.0},{:.0}) screen=({:.0},{:.0})\n  {tap_log}\n  \
         order after tap: {order_desc}\n  worker moved {:.1}u",
        mine.x,
        mine.y,
        screen.x,
        screen.y,
        pos_after.distance(worker.pos)
    );

    match order {
        Order::Harvest { mine: target, .. } => assert!(
            target.distance(mine) <= 1.0,
            "economy.gold: the harvest target ({:.0},{:.0}) is not the tapped mine ({:.0},{:.0}).\n  {tap_log}",
            target.x,
            target.y,
            mine.x,
            mine.y
        ),
        Order::HarvestLumber { tree, .. } => panic!(
            "economy.gold: the tap on the mine at ({:.0},{:.0}) ordered a LUMBER harvest of the tree at \
             ({:.0},{:.0}).\n  {tap_log}\n  order: {order_desc}",
            mine.x, mine.y, tree.x, tree.y
        ),
        Order::Idle | Order::Move(_) | Order::MoveAttack(_) => panic!(
            "economy.gold: TOUCH INTENT NOT REACHED — a tap on the mine ({:.0},{:.0}) produced `{order_desc}`.\n  \
             Either the tap was consumed by a picked unit (`picked=` above), or `near_screen` missed \
             (radius 72*layout.scale={:.0}px).\n  {tap_log}",
            mine.x,
            mine.y,
            72.0 * game.layout.scale
        ),
        other => panic!(
            "economy.gold: tap on the mine produced an unexpected order `{}`.\n  {tap_log}",
            order_text(other)
        ),
    }

    let budget = trip_budget(&game, &worker, mine);
    eprintln!("economy.gold: 10 gold/trip, budget {budget} ticks");
    let mut trace = Vec::new();
    let mut errors = Vec::new();
    let start_tick = game.sim.tick;
    let mut picked_tick = None;
    let mut deposited_tick = None;
    let mut tick = 0u32;
    while tick < budget {
        tick += 1;
        let tick_now = game.sim.tick - start_tick;
        advance(&mut game, worker.id, tick_now, &mut trace, &mut errors);
        let unit = game.sim.unit(worker.id).expect("worker alive");
        if picked_tick.is_none() && unit.carrying > 0 {
            picked_tick = Some(game.sim.tick - start_tick);
        }
        if game.sim.resources[player] > gold0 {
            deposited_tick = Some(game.sim.tick - start_tick);
            break;
        }
        if unit.hp <= 0 {
            break;
        }
    }
    let gold1 = game.sim.resources[player];
    let lumber1 = game.sim.lumber[player];
    let unit = game.sim.unit(worker.id).expect("worker alive");
    eprintln!(
        "PARITY economy.gold touch_tap: gold {gold0}->{gold1} lumber {lumber0}->{lumber1} \
         picked_at={picked_tick:?} deposited_at={deposited_tick:?} tick={} order={}",
        game.sim.tick - start_tick,
        order_text(unit.order)
    );
    if gold1 <= gold0 {
        let trip = if picked_tick.is_some() {
            "the worker mined a load but never deposited it: inspect ToHall pathing/deposit radius (160u)"
        } else if unit.pos.distance(worker.pos) < 8.0 {
            "the worker never moved: the Harvest movement leg is not running (order_movement/ToTarget)"
        } else {
            "the worker moved but never reached Gathering: inspect the 56u mine stop radius"
        };
        panic!(
            "economy.gold: tapping a mine did not pay gold ({gold0}->{gold1}, lumber {lumber0}->{lumber1}).\n  \
             CAUSE CLASS: {trip}\n  \
             state: order={} pos=({:.0},{:.0}) path={} carry={} timer={}\n  \
             sim errors: {:?}\n  last trace:\n{}\n  {tap_log}",
            order_text(unit.order),
            unit.pos.x,
            unit.pos.y,
            unit.path.len(),
            unit.carrying,
            unit.harvest_timer,
            errors,
            trace_text(&trace, 14)
        );
    }
    assert!(
        picked_tick.is_some(),
        "economy.gold: gold rose but no carry was observed — deposit came from another source"
    );
}
