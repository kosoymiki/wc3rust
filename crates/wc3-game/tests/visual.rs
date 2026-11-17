//! Automated visual regression: render the real map headlessly and reject the
//! artifacts the first playable builds actually showed.
use std::path::Path;

use wc3_game::{render, Assets, Game};

fn analyze(rgba: &[u8], width: u32, height: u32) -> (f64, f64, f64, f64, usize) {
    let world_top = (height as f64 * 0.14) as u32;
    let world_bottom = (height as f64 * 0.72) as u32;
    let world_left = (width as f64 * 0.08) as u32;
    let world_right = (width as f64 * 0.92) as u32;
    let mut green = 0u64;
    let mut black = 0u64;
    let mut white = 0u64;
    let mut blue = 0u64;
    let mut total = 0u64;
    let mut buckets = std::collections::HashSet::new();
    for y in world_top..world_bottom {
        for x in world_left..world_right {
            let index = ((y * width + x) * 4) as usize;
            let (r, g, b) = (rgba[index] as i32, rgba[index + 1] as i32, rgba[index + 2] as i32);
            total += 1;
            if g > r + 8 && g > b + 4 && g > 40 {
                green += 1;
            }
            if r + g + b < 36 {
                black += 1;
            }
            if r > 232 && g > 232 && b > 232 {
                white += 1;
            }
            if b > r + 18 && b > g + 6 && b > 40 {
                blue += 1;
            }
            buckets.insert(((r / 24) as u8, (g / 24) as u8, (b / 24) as u8));
        }
    }
    let total = total.max(1) as f64;
    (
        green as f64 / total,
        black as f64 / total,
        white as f64 / total,
        blue as f64 / total,
        buckets.len(),
    )
}

#[test]
fn real_map_renders_terrain_units_and_no_giant_artifacts() {
    let mpq = Path::new("/data/data/com.termux/files/home/src/wc3rust/analysis/war3_iso/War3.mpq");
    let mpq = if mpq.exists() {
        mpq.to_path_buf()
    } else {
        Path::new("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq").to_path_buf()
    };
    let map = Path::new("/data/data/com.termux/files/home/src/wc3rust/analysis/war3_map/Maps/Campaign/Human04.w3m");
    if !mpq.exists() || !map.exists() {
        eprintln!("SKIP visual: user War3.mpq/map not present");
        return;
    }
    let assets = Assets::load(Some(&mpq), Some(map));
    assert_eq!(assets.source, "user-mpq");
    let mut game = Game::new(assets, 0x5EED);
    // The boot menu paints a full-screen backdrop; the visual gate renders the
    // battle, so enter it explicitly (same hook the other headless tests use).
    game.enter_battle();
    game.resize(960.0, 540.0);
    for _ in 0..30 {
        game.update(1.0 / 30.0);
    }
    let scene = game.scene();
    let rgba = match render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        960,
        540,
    ) {
        Ok(rgba) => rgba,
        Err(error) => {
            eprintln!("SKIP visual: headless render unavailable: {error}");
            return;
        }
    };
    render::write_ppm("analysis/visual-real-map.ppm", 960, 540, &rgba);
    let (green, black, white, blue, colours) = analyze(&rgba, 960, 540);
    println!("VISUAL green={green:.3} black={black:.3} white={white:.3} blue={blue:.3} colours={colours}");
    assert!(green > 0.18, "terrain should be green-dominant, got {green:.3}");
    assert!(black < 0.20, "no black texture holes, got {black:.3}");
    assert!(white < 0.10, "no giant white artifact, got {white:.3}");
    assert!(blue > 0.0015, "player units should be visible, got {blue:.4}");
    assert!(colours > 40, "scene needs real texture variety, got {colours}");
}
