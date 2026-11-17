//! Diagnostic probe (tech-lead): does a tap on a gold mine / tree produce a
//! real gather order and income on the installed Justice map?
use glam::Vec3;
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;
use wc3_sim::Vec2;

fn tap(game: &mut Game, world: Vec2) {
    let ground = game.assets.height_at(world.x, world.y);
    let Some(screen) = game
        .camera
        .world_to_screen(Vec3::new(world.x, ground + 30.0, world.y))
    else {
        eprintln!("  (off screen: {world:?})");
        return;
    };
    game.pointer_down(screen);
    game.pointer_up(screen);
    eprintln!(
        "  tap screen=({:.0},{:.0}) pending={} sel={}",
        screen.x,
        screen.y,
        0usize, // pending is private
        game.selection.len()
    );
}

fn main() {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ]
    .iter()
    .map(Path::new)
    .find(|p| p.is_file());
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    let Some(mpq) = mpq else {
        println!("no install");
        return;
    };
    let mut game = Game::new(Assets::load(Some(mpq), Some(&map)), 0x5EED);
    game.resize(2414.0, 1080.0);
    let player = game.player;
    let peon = game
        .sim
        .alive_units()
        .find(|u| u.owner == player && u.def().stats.is_worker)
        .map(|u| (u.id, u.pos, u.def().id))
        .expect("worker");
    println!("peon {:?} def={} pos={:?}", peon.0, peon.2, peon.1);

    // select the peon
    tap(&mut game, peon.1);
    println!("  selected={:?}", game.selection);
    for _ in 0..3 {
        game.update(1.0 / 30.0);
    }

    // mine tap
    let mine = game.sim.nearest_gold_mine(peon.1).expect("mine");
    game.camera.centre_on(mine);
    for _ in 0..2 {
        game.update(1.0 / 30.0);
    }
    let gold0 = game.sim.resources[player as usize];
    println!("mine at {mine:?} gold={gold0}");
    tap(&mut game, mine);
    eprintln!("  order right after mine tap: {:?}", game.sim.unit(peon.0).map(|u| u.order.clone()));
    for _ in 0..3 {
        game.update(1.0 / 30.0);
    }
    eprintln!("  order 3 ticks after mine tap: {:?}", game.sim.unit(peon.0).map(|u| (u.order.clone(), u.goal, u.path.len())));
    for _ in 0..900 {
        game.update(1.0 / 30.0);
    }
    let gold1 = game.sim.resources[player as usize];
    println!("  gold {gold0} -> {gold1} (order={:?})", game.sim.unit(peon.0).map(|u| (u.order.clone(), u.goal)));

    // tree tap
    let tree = game
        .sim
        .destructables
        .iter()
        .filter(|t| t.alive)
        .min_by(|a, b| {
            (a.pos - mine)
                .length()
                .partial_cmp(&(b.pos - mine).length())
                .unwrap()
        })
        .map(|t| (t.pos, t.def_index))
        .expect("tree");
    game.camera.centre_on(tree.0);
    for _ in 0..2 {
        game.update(1.0 / 30.0);
    }
    let lum0 = game.sim.lumber[player as usize];
    println!("tree at {:?} lumber={lum0}", tree.0);
    tap(&mut game, tree.0);
    eprintln!("  order right after tree tap: {:?}", game.sim.unit(peon.0).map(|u| u.order.clone()));
    for _ in 0..3 {
        game.update(1.0 / 30.0);
    }
    eprintln!("  order 3 ticks after tree tap: {:?}", game.sim.unit(peon.0).map(|u| (u.order.clone(), u.goal, u.path.len())));
    for _ in 0..1800 {
        game.update(1.0 / 30.0);
    }
    let lum1 = game.sim.lumber[player as usize];
    println!("  lumber {lum0} -> {lum1} (order={:?})", game.sim.unit(peon.0).map(|u| (u.order.clone(), u.goal)));
}
