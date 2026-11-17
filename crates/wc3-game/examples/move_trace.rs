//! Trace: does a walking unit keep the Walk clip, and how far does it move per tick?
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;
use wc3_sim::Command;

fn main() {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.herofolends.p0/files/War3.mpq",
    ]
    .iter()
    .map(Path::new)
    .find(|p| p.is_file());
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    let Some(mpq) = mpq else { println!("no install"); return };
    let mut game = Game::new(Assets::load(Some(mpq), Some(&map)), 0x5EED);
    game.enter_battle();
    let worker = game
        .sim
        .alive_units()
        .find(|u| u.owner == game.player && u.def().stats.is_worker)
        .map(|u| u.id)
        .expect("worker");
    let from = game.sim.unit(worker).unwrap().pos;
    let to = from + wc3_sim::Vec2::new(1200.0, 600.0);
    game.issue(Command::Move { units: vec![worker], to });

    let mut last = from;
    let mut walk_ticks = 0;
    let mut idle_while_moving = 0;
    for i in 0..90 {
        game.update(1.0 / 30.0);
        let Some(unit) = game.sim.unit(worker) else { break };
        let moved = (unit.pos - last).length();
        last = unit.pos;
        let seq = game.unit_sequence_name(worker).unwrap_or("?").to_string();
        let lower = seq.to_ascii_lowercase();
        if lower.contains("walk") {
            walk_ticks += 1;
        }
        if moved > 1.0 && !lower.contains("walk") {
            idle_while_moving += 1;
        }
        if i < 18 || i % 10 == 0 {
            println!(
                "t={:>3} tick={:>4} moved={:5.2} path={:<3} order={:?} seq={seq}",
                i,
                game.sim.tick,
                moved,
                unit.path.len(),
                unit.order
            );
        }
    }
    println!("walk_ticks={walk_ticks} idle_while_moving={idle_while_moving}");
}
