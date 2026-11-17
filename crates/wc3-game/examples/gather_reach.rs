//! Gather reachability on the real map: does a worker reach every gold mine
//! after the footprints became real (a Great Hall now blocks 6x6 cells)?
//!
//! Operator frame showed `GATHER UNREACHABLE`, so this measures it:
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example gather_reach --offline
//! ```

use std::path::Path;

use glam::Vec2;
use wc3_game::{Assets, Game};
use wc3_sim::Command;

const MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn main() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(MAP);
    if !path.is_file() {
        println!("GATHER_REACH skip: map missing");
        return;
    }
    let assets = Assets::load(None, Some(&path));
    let mut game = Game::new(assets, 0x5EED);
    game.enter_battle();

    let mines: Vec<Vec2> = game.sim.gold_mines.iter().map(|m| m.pos).collect();
    println!("GATHER_REACH mines={}", mines.len());
    let worker = game
        .sim
        .alive_units()
        .find(|u| u.owner == game.player && u.def().stats.is_worker)
        .map(|u| u.id);
    let Some(worker) = worker else {
        println!("GATHER_REACH no worker");
        return;
    };

    for (index, mine) in mines.iter().enumerate() {
        // Walkable cells in a 9x9 window around the mine: 0 means it is sealed.
        let cell = game.sim.grid.world_to_cell(*mine);
        let mut open = 0usize;
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let x = cell.0 as i32 + dx;
                let y = cell.1 as i32 + dy;
                if x >= 0
                    && y >= 0
                    && (x as u32) < game.sim.grid.width
                    && (y as u32) < game.sim.grid.height
                    && game.sim.grid.walkable(x as u32, y as u32)
                {
                    open += 1;
                }
            }
        }
        // Teleport-free test: the worker keeps its position, the order is the
        // only thing issued; the sim reports `gather unreachable` if no path.
        let pos = game.sim.unit(worker).map(|u| u.pos).unwrap_or(Vec2::ZERO);
        game.sim.step(&[Command::Harvest {
            units: vec![worker],
            to: *mine,
        }]);
        let mut unreachable = false;
        for _ in 0..240 {
            game.sim.step(&[]);
            if game
                .sim
                .events
                .iter()
                .rev()
                .take(8)
                .any(|event| format!("{event:?}").to_lowercase().contains("unreachable"))
            {
                unreachable = true;
                break;
            }
        }
        let after = game.sim.unit(worker).map(|u| u.pos).unwrap_or(Vec2::ZERO);
        let carrying = game
            .sim
            .unit(worker)
            .map(|u| u.carrying_lumber > 0)
            .unwrap_or(false);
        println!(
            "MINE {index} at=({:.0},{:.0}) dist_from_worker={:.0} open_cells_9x9={open} moved={:.0} carrying={carrying} unreachable={unreachable}",
            mine.x,
            mine.y,
            pos.distance(*mine),
            pos.distance(after)
        );
    }
}
