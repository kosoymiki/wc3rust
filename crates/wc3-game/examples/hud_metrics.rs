//! Diagnostic (tech-lead): card text budget vs slot size at device resolution.
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;
use wc3_game::hud::Font;

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
    game.resize(2414.0, 1080.0);
    let worker = game
        .sim
        .alive_units()
        .find(|u| u.owner == game.player && u.def().stats.is_worker)
        .map(|u| u.id)
        .expect("worker");
    // Real tap path so the card rebuilds exactly as on the device.
    let unit = game.sim.unit(worker).unwrap();
    let ground = game.assets.height_at(unit.pos.x, unit.pos.y);
    let screen = game
        .camera
        .world_to_screen(glam::Vec3::new(unit.pos.x, ground + 40.0, unit.pos.y))
        .expect("on screen");
    game.pointer_down(screen);
    game.pointer_up(screen);
    for _ in 0..3 {
        game.update(1.0 / 30.0);
    }
    println!("selected={} menu={} buttons={}", game.selection.len(), game.menu.len(), game.buttons.len());
    let font = Font::load();
    println!(
        "viewport=2414x1080 layout_scale={:.3} slot={:.1}px font atlas line_height={:.1} ascent={:.1} source={:?}",
        game.layout.scale,
        game.layout.button,
        font.line_height,
        font.ascent,
        font.source
    );
    let cov = font.coverage();
    println!("glyphs covered={} missing={} ascii={} cyrillic={}", cov.covered, cov.missing, cov.ascii_covered, cov.cyrillic_covered);
    let slot = game.layout.button;
    for (i, button) in game.buttons.iter().enumerate().take(12) {
        if button.label.is_empty() {
            continue;
        }
        let s_intended = 0.42 * game.layout.scale;
        let s_floor = (0.42 * game.layout.scale).max(wc3_game::hud::MIN_TEXT_PX / 30.0);
        let w_intended = font.measure(&button.label, s_intended);
        let w_floor = font.measure(&button.label, s_floor);
        let w_hot = font.measure(&button.hotkey, s_floor);
        println!(
            "slot{i:<2} label={:<22} hot={:<3} cost={:<8} slot_w={slot:.0} | w@0.42={w_intended:.0} ({:.0}%) w@floor={w_floor:.0} ({:.0}%) hotkey_w={w_hot:.0}",
            button.label,
            button.hotkey,
            button.sub,
            w_intended / slot * 100.0,
            w_floor / slot * 100.0
        );
    }
    println!(
        "vertical budget: slot_h={slot:.0} line@floor={:.0} two_lines+hotkey+cost={:.0}",
        font.line_height * (wc3_game::hud::MIN_TEXT_PX / 30.0),
        font.line_height * (wc3_game::hud::MIN_TEXT_PX / 30.0) * 3.0
    );
}
