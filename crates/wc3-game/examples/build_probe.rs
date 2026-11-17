//! Placement/terrain probe on the real melee map: how much of Justice is
//! buildable, and does an atomic placement actually complete.
//!
//! Operator report: «здания не строятся, какие-то проблемы с ландшафтом».
//! This turns that into numbers: walkable fraction, `can_place` rate around the
//! player's start, and the construction lifecycle over 900 ticks.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example build_probe --offline
//! ```

use std::path::Path;
use wc3_game::{Assets, Game};
use glam::vec2;
use wc3_sim::UnitKind;

const MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn main() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(MAP);
    if !path.is_file() {
        println!("BUILD_PROBE skip: map missing at {}", path.display());
        return;
    }
    let assets = Assets::load(None, Some(&path));
    let mut game = Game::new(assets, 0x5EED);
    game.enter_battle();

    let sim = &game.sim;
    let (gw, gh) = (sim.grid.width, sim.grid.height);
    let mut walkable = 0usize;
    for y in 0..gh {
        for x in 0..gw {
            if sim.grid.walkable(x, y) {
                walkable += 1;
            }
        }
    }
    let total = (gw * gh) as usize;
    println!(
        "GRID {gw}x{gh} cells={total} walkable={walkable} ({:.1}%)",
        walkable as f32 / total as f32 * 100.0
    );

    // Sample the whole playable rectangle.
    let mut sampled = 0usize;
    let mut legal = 0usize;
    let mut blocked_cells = 0usize;
    for iy in 0..60 {
        for ix in 0..60 {
            let p = vec2(
                gw as f32 * sim.grid.cell_size * (ix as f32 + 0.5) / 60.0,
                gh as f32 * sim.grid.cell_size * (iy as f32 + 0.5) / 60.0,
            );
            sampled += 1;
            let cell = sim.grid.world_to_cell(p);
            if !sim.grid.walkable(cell.0, cell.1) {
                blocked_cells += 1;
                continue;
            }
            if sim.can_place("htow", p) {
                legal += 1;
            }
        }
    }
    println!(
        "SAMPLE {sampled} points: unwalkable={blocked_cells} ({:.1}%) can_place(htow)={legal}",
        blocked_cells as f32 / sampled as f32 * 100.0
    );

    // Around the player's own start: the base must be buildable.
    let start = sim
        .alive_units()
        .find(|u| u.owner == game.player)
        .map(|u| u.pos)
        .unwrap_or(vec2(gw as f32 * 16.0, gh as f32 * 16.0));
    let mut near = 0usize;
    let mut near_legal = 0usize;
    for iy in 0..24 {
        for ix in 0..24 {
            let p = vec2(
                start.x - 600.0 + 1200.0 * (ix as f32 + 0.5) / 24.0,
                start.y - 600.0 + 1200.0 * (iy as f32 + 0.5) / 24.0,
            );
            near += 1;
            if sim.can_place("htow", p) {
                near_legal += 1;
            }
        }
    }
    println!(
        "NEAR_START radius=600 sampled={near} can_place={near_legal} ({:.1}%)",
        near_legal as f32 / near as f32 * 100.0
    );

    // Atomic placement + construction lifecycle.
    let worker = sim
        .alive_units()
        .find(|u| u.owner == game.player && u.def().stats.is_worker)
        .map(|u| u.id);
    let Some(worker) = worker else {
        println!("BUILD_PROBE no worker");
        return;
    };
    let mut site = None;
    'outer: for iy in 0..40 {
        for ix in 0..40 {
            let p = vec2(
                start.x - 500.0 + 1000.0 * (ix as f32 + 0.5) / 40.0,
                start.y - 500.0 + 1000.0 * (iy as f32 + 0.5) / 40.0,
            );
            if game.sim.can_place("htow", p) {
                site = Some(p);
                break 'outer;
            }
        }
    }
    let Some(site) = site else {
        println!("BUILD_PROBE NO_LEGAL_SITE within 500 units of start");
        return;
    };
    println!("SITE ({:.0},{:.0})", site.x, site.y);
    match game.sim.build_instant(worker, UnitKind::TownHall, site) {
        Ok(id) => {
            let started = game.sim.unit(id).and_then(|u| u.construct.clone());
            let frac0 = started
                .as_ref()
                .map(|c| 1.0 - c.remaining as f32 / c.total.max(1) as f32)
                .unwrap_or(1.0);
            println!("BUILD_OK unit={id:?} construct={:?} progress={frac0:.2}", started.is_some());
            for _ in 0..900 {
                game.update(1.0 / 30.0);
            }
            if let Some(u) = game.sim.unit(id) {
                let frac = u
                    .construct
                    .as_ref()
                    .map(|c| 1.0 - c.remaining as f32 / c.total.max(1) as f32)
                    .unwrap_or(1.0);
                println!(
                    "AFTER_900_TICKS alive={} progress={frac:.2} hp={}/{} construct={}",
                    u.alive,
                    u.hp,
                    u.max_hp,
                    u.construct.is_some()
                );
            } else {
                println!("AFTER_900_TICKS unit gone");
            }
        }
        Err(reason) => println!("BUILD_ERR {reason}"),
    }
}
