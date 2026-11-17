//! Diagnostic: per-def census of the scene that `Game::new` actually spawns.
use std::collections::BTreeMap;
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;

fn main() {
    let root = Path::new("/data/data/org.wc3rust.p0/files");
    let map = root.join("Melee.w3x");
    if !map.is_file() {
        println!("skip: {map:?} missing");
        return;
    }
    let game = Game::new(Assets::load(Some(&root.join("War3.mpq")), Some(&map)), 0x5EED);
    let mut by_def: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut buildings = 0usize;
    for unit in game.sim.alive_units() {
        *by_def.entry(unit.def().id).or_default() += 1;
        if unit.is_building() {
            buildings += 1;
        }
    }
    println!(
        "melee={} alive={} buildings={} mines={} camps={} destructables={}",
        game.melee,
        game.sim.alive_units().count(),
        buildings,
        game.sim.gold_mines.len(),
        game.sim.camps.len(),
        game.sim.destructables.len()
    );
    for (def, count) in &by_def {
        println!("  {def:<6} x{count}");
    }
    // Neutral placement audit: where the map's creeps actually are relative to
    // the player start. A murloc next to the base is map content, not a spawn
    // bug — unless its authored position is far away.
    {
        let start = game.base_pos;
        println!("player base {start:?}");
        for unit in game.sim.alive_units() {
            let def = unit.def().id;
            if def.starts_with('n') && !unit.def().stats.is_building && !unit.def().stats.is_worker {
                let d = (unit.pos - start).length();
                if d < 2500.0 {
                    println!(
                        "  near-base neutral {def} at {:?} d={d:.0} owner={}",
                        unit.pos, unit.owner
                    );
                }
            }
        }
    }
}
