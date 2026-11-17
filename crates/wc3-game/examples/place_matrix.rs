//! Per-def placement matrix on the real map: how much of the ground around the
//! player's base accepts each buildable building.
//!
//! Operator report: «остальные здания не размещаются на ландшафте» (and the
//! probe managed to stack three halls on one spot, which means the site's
//! footprint is not reserved). Both are measured here:
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example place_matrix --offline
//! ```

use std::path::Path;

use glam::Vec2;
use wc3_game::{Assets, Game};
use wc3_sim::data;

const MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn main() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(MAP);
    if !path.is_file() {
        println!("PLACE_MATRIX skip: map missing");
        return;
    }
    let assets = Assets::load(None, Some(&path));
    let mut game = Game::new(assets, 0x5EED);
    game.enter_battle();

    let base = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.is_building())
        .map(|unit| unit.pos)
        .unwrap_or(Vec2::ZERO);
    println!(
        "PLACE_MATRIX base=({:.0},{:.0}) cell_size={} grid={}x{} world={:?}",
        base.x, base.y, game.sim.grid.cell_size, game.sim.grid.width, game.sim.grid.height,
        game.assets.world_size
    );

    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id);

    // 24x24 grid over 1200x1200 px around the base = 576 candidate points.
    let mut points = Vec::new();
    for iy in 0..24 {
        for ix in 0..24 {
            points.push(Vec2::new(
                base.x - 600.0 + 1200.0 * (ix as f32 + 0.5) / 24.0,
                base.y - 600.0 + 1200.0 * (iy as f32 + 0.5) / 24.0,
            ));
        }
    }

    println!(
        "{:<10} {:>4} {:>6} {:>8} {:>8}  notes",
        "def", "fp", "cost", "legal", "percent"
    );
    let mut buildable: Vec<&'static data::UnitDef> = data::ROSTER
        .iter()
        .filter(|def| def.is_building() && def.buildable && def.race == wc3_sim::data::Race::Orc)
        .collect();
    buildable.sort_by_key(|def| def.id);

    let mut total_legal = 0usize;
    for def in buildable {
        let legal = points
            .iter()
            .filter(|p| game.sim.can_place(def.id, **p))
            .count();
        total_legal += legal;
        println!(
            "{:<10} {:>4} {:>6} {:>8} {:>7.1}%  {}",
            def.id,
            def.footprint,
            def.stats.cost,
            legal,
            legal as f32 / points.len() as f32 * 100.0,
            if legal == 0 { "NOTHING PLACEABLE" } else { "" }
        );
    }

    // Stacking check: place two sites at the same point and see whether the
    // second is refused (it must be: the footprint is reserved).
    if let Some(worker) = worker {
        let spot = points
            .iter()
            .copied()
            .find(|p| game.sim.can_place("ogre", *p));
        if let Some(spot) = spot {
            let first = game.sim.place_building_ordered(worker, "ogre", spot);
            let legal_after_first = game.sim.can_place("ogre", spot);
            let second = game.sim.place_building_ordered(worker, "ogre", spot);
            let offset = spot + Vec2::new(96.0, -96.0);
            let third = game.sim.place_building_ordered(worker, "ogre", offset);
            if let Ok(id) = first {
                if let Some(site) = game.sim.unit(id) {
                    println!("STACK cells={:?}", site.blocked_cells);
                }
            }
            println!(
                "STACK spot_cell={:?} offset_cell={:?} legal_after_first={legal_after_first} third_offset_err={:?}",
                game.sim.grid.world_to_cell(spot),
                game.sim.grid.world_to_cell(offset),
                third.as_ref().err()
            );
            println!(
                "STACK_CHECK first={:?} second={:?} legal_now={}",
                first.is_ok(),
                second.is_ok(),
                game.sim.can_place("ogre", spot)
            );
        }
    }
    println!("PLACE_MATRIX done total_legal={total_legal}");
}
