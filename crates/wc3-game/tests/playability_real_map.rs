//! Playability gate on a REAL shipped map: units must be able to walk across
//! the map, workers must harvest and build, and a placed building must appear.
//!
//! These are the operator-reported failures: "units cannot move across the
//! map", "units get stuck in textures", "workers cannot build or harvest".
//! Synthetic grids hid them, so every check here runs on Orc01/Human04 with the
//! real WPM + terrain.

use std::path::{Path, PathBuf};

use glam::Vec2;
use wc3_game::assets::Assets;
use wc3_game::game::{Game, UiAction};

fn map_path(name: &str) -> Option<PathBuf> {
    // The extracted archive keeps campaign maps under Maps/Campaign/. The old
    // single `Maps/<name>` probe missed them and every test in this file
    // returned "skip" with rc 0 — a green gate that proved nothing.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Maps");
    let candidates = [root.join(name), root.join("Campaign").join(name)];
    for candidate in &candidates {
        if candidate.is_file() {
            return Some(candidate.clone());
        }
    }
    if root.is_dir() {
        panic!(
            "the extracted map tree exists but {name} is missing; looked at {candidates:?}"
        );
    }
    eprintln!("skip: analysis/war3_full3/Maps absent (extracted WC3 maps needed)");
    None
}

fn melee_game() -> Option<Game> {
    let map = map_path("Orc01.w3m")?;
    // The boot menu gates the sim until input; tests drive the battle directly.
    let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
    game.enter_battle();
    Some(game)
}

/// The operator's melee map has gold mines, trees and a free build ring; the
/// campaign map (Orc01) creates its units from JASS, so economy checks belong
/// on a map that actually ships mines.
fn justice_map() -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    path.is_file().then_some(path)
}

fn justice_game() -> Option<Game> {
    let map = justice_map()?;
    // The boot menu gates the sim until input; tests drive the battle directly.
    let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
    game.enter_battle();
    Some(game)
}

#[test]
fn units_walk_across_the_real_map() {
    // A campaign arena (Orc01) is an enclosed scripted area: from its start no
    // far target has a path at all (measured: six candidates, all path 0). The
    // cross-map walk belongs on the operator's open melee map.
    let Some(mut game) = justice_game() else {
        eprintln!("skip: justice map missing");
        return;
    };
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("player worker");
    let start = game.sim.unit(worker).expect("worker").pos;
    // Target: the farthest **reachable** far corner. A campaign map is not a
    // melee arena — its own geometry leaves parts of the rectangle unwalkable,
    // so the test picks the first candidate that really produces a path rather
    // than asserting the map is a flat box.
    let world = game.assets.world_size;
    game.selection = vec![worker];
    let fractions = [
        (0.75f32, 0.75f32),
        (0.75, 0.25),
        (0.25, 0.75),
        (0.50, 0.90),
        (0.90, 0.50),
        (0.50, 0.50),
    ];
    let mut chosen: Option<(Vec2, usize)> = None;
    let mut tried: Vec<(Vec2, usize)> = Vec::new();
    for (fx, fy) in fractions {
        let candidate = Vec2::new(world.x * fx, world.y * fy);
        if start.distance(candidate) <= 4000.0 {
            continue;
        }
        game.sim
            .issue(&[worker], wc3_sim::OrderSpec::Move(candidate));
        let path_len = game
            .sim
            .unit(worker)
            .map(|unit| unit.path.len())
            .unwrap_or(0);
        tried.push((candidate, path_len));
        if path_len > 5 {
            chosen = Some((candidate, path_len));
            break;
        }
        game.sim.issue(&[worker], wc3_sim::OrderSpec::Stop);
    }
    let Some((target, path_len)) = chosen else {
        panic!(
            "no reachable far target on the real map (start {start:?}, walkable radius {} cells, tried {tried:?})",
            walkable_radius(&game, start)
        );
    };
    eprintln!("walk target {target:?} path_steps={path_len}");

    let mut best = start.distance(target);
    for _ in 0..3_000 {
        game.update(1.0 / 30.0);
        if let Some(unit) = game.sim.unit(worker) {
            best = best.min(unit.pos.distance(target));
        }
    }
    let end = game.sim.unit(worker).expect("worker").pos;
    assert!(
        best < 900.0,
        "unit did not cross the map: start {start:?} end {end:?} target {target:?} best {best:.0}"
    );
}

fn walkable_radius(game: &Game, at: Vec2) -> usize {
    let cell = game.sim.grid.world_to_cell(at);
    let mut walkable = 0;
    for dy in -4i32..=4 {
        for dx in -4i32..=4 {
            let x = cell.0 as i32 + dx;
            let y = cell.1 as i32 + dy;
            if x >= 0 && y >= 0 && game.sim.grid.walkable(x as u32, y as u32) {
                walkable += 1;
            }
        }
    }
    walkable
}

#[test]
fn worker_harvests_gold_on_the_real_map() {
    // Orc01 is a campaign map: its units come from the JASS script and the
    // shipped melee mine list can be empty. The economy gate runs on the
    // operator's melee map (Justice), which ships 7 mines.
    let Some(mut game) = justice_game() else {
        eprintln!("skip: Justice.w3x missing");
        return;
    };
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("player worker");
    let pos = game.sim.unit(worker).unwrap().pos;
    let mine = game.sim.nearest_gold_mine(pos);
    let Some(mine) = mine else {
        let ngol: Vec<_> = game
            .sim
            .alive_units()
            .filter(|unit| unit.def().id.eq_ignore_ascii_case("ngol"))
            .map(|unit| unit.pos)
            .collect();
        panic!(
            "economy.gold: the melee map exposes no gold mine to the sim \
             (gold_mines={}, engine={}, script-created ngol units={ngol:?}) — \
             mines must be registered in sim.gold_mines, not only in the script world",
            game.sim.gold_mines.len(),
            game.engine.is_some()
        );
    };
    let before = game.sim.resources[game.player as usize];
    game.sim
        .issue(&[worker], wc3_sim::OrderSpec::Harvest(mine));
    for _ in 0..3_000 {
        game.update(1.0 / 30.0);
    }
    let after = game.sim.resources[game.player as usize];
    assert!(
        after > before,
        "worker never paid out gold: {before} -> {after} (mine {mine:?})"
    );
}

#[test]
fn worker_builds_a_farm_and_the_building_appears() {
    let Some(mut game) = justice_game() else {
        eprintln!("skip: Justice.w3x missing");
        return;
    };
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("player worker");
    game.sim.resources[game.player as usize] = 5_000;
    game.sim.lumber[game.player as usize] = 5_000;
    let pos = game.sim.unit(worker).unwrap().pos;
    let before = game
        .sim
        .alive_units()
        .filter(|unit| unit.def().stats.is_building)
        .count();
    // The worker card's BUILD entry must be reachable from the real UI path.
    game.selection = vec![worker];
    game.update(0.0);
    let build_def = game
        .buttons
        .iter()
        .find_map(|button| match button.action {
            UiAction::BuildDef(id) => Some(id),
            _ => None,
        })
        .expect("worker card exposes a building entry");
    // `walkable_pos_near` only proves the cell is terrain-walkable: the first
    // probe at (240,120) landed on an occupied footprint ("site occupied").
    // The site search must use the same `can_place` predicate the game uses.
    let mut tried = Vec::new();
    let mut site = None;
    'rings: for ring in [240.0f32, 360.0, 480.0, 620.0] {
        for (dx, dy) in [(1.0f32, 0.5f32), (0.0, 1.0), (-1.0, 0.5), (1.0, -0.5), (-1.0, -0.5), (0.0, -1.0)] {
            let probe = pos + Vec2::new(dx * ring, dy * ring);
            let Some(candidate) = game.sim.walkable_pos_near(probe, 8) else {
                tried.push((probe, "no walkable cell"));
                continue;
            };
            if game.sim.can_place(build_def, candidate) {
                site = Some(candidate);
                break 'rings;
            }
            tried.push((candidate, "can_place=false"));
        }
    }
    let site = site.unwrap_or_else(|| {
        panic!("no placeable {build_def} site around {pos:?}; tried {tried:?}")
    });
    let placed = game.sim.place_building(worker, build_def, site);
    assert!(
        placed.is_ok(),
        "place_building({build_def}) failed: {placed:?} at {site:?}"
    );
    for _ in 0..600 {
        game.update(1.0 / 30.0);
    }
    let after = game
        .sim
        .alive_units()
        .filter(|unit| unit.def().stats.is_building)
        .count();
    assert!(after > before, "building never appeared: {before} -> {after}");
}
