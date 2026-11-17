//! Headless 2414x1080 day/night evidence frames.
//!
//! Renders the real Human04 map through the game facade at three clock hours
//! and writes PNGs to `analysis/hudskin/daynight-<phase>-2414x1080.png`.
//!
//! The hours come from the shipped DNC tables, not taste:
//! `DNCLordaeronTerrain.mdx` ramps from the warm night colour at ~05:20 to the
//! full-day white key exactly at 06:00 (`KLAC` key t=15333 ms of the 60 000 ms
//! `Stand` sequence), so a literal 06:00 frame is the same light as noon. The
//! primary `dawn` capture therefore samples the middle of that ramp (05:30);
//! a literal 06:00 frame is written next to it as `daynight-dawn0600-*` so the
//! claim is checkable against the data.
use std::path::{Path, PathBuf};

use wc3_game::{Assets, Game, render};

const WIDTH: u32 = 2414;
const HEIGHT: u32 = 1080;
const OUT_DIR: &str = "analysis/hudskin";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn install_mpq() -> Option<PathBuf> {
    let root = repo();
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("H2_WC3_INSTALL") {
        candidates.push(PathBuf::from(dir).join("War3x.mpq"));
    }
    candidates.push(root.join("analysis/agent-assets/wc3-install/War3x.mpq"));
    candidates.push(root.join("analysis/tft_files_Files100_War3x.mpq"));
    candidates.push(PathBuf::from(
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3x.mpq",
    ));
    candidates.into_iter().find(|path| path.is_file())
}

fn map_path() -> Option<PathBuf> {
    let root = repo();
    [
        root.join("analysis/war3_map/Maps/Campaign/Human04.w3m"),
        root.join(
            "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        ),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn game() -> Option<Game> {
    let mpq = install_mpq()?;
    let map = map_path()?;
    let assets = Assets::load(Some(&mpq), Some(map.as_path()));
    eprintln!(
        "DAYNIGHT assets mpq={} map={} source={} models={}",
        mpq.display(),
        map.display(),
        assets.source,
        assets.models.len()
    );
    let mut game = Game::new(assets, 0x5EED);
    game.enter_battle();
    game.resize(WIDTH as f32, HEIGHT as f32);
    for _ in 0..30 {
        game.update(1.0 / 30.0);
    }
    Some(game)
}

/// Mean luminance (0..255) of the world band; the HUD keeps its own
/// brightness in WC3, so the lighting comparison excludes it.
fn world_luminance(rgba: &[u8], width: u32, height: u32) -> (f64, f64) {
    let top = (height as f64 * 0.08) as u32;
    let bottom = (height as f64 * 0.70) as u32;
    let left = (width as f64 * 0.06) as u32;
    let right = (width as f64 * 0.94) as u32;
    let mut sum = 0.0f64;
    let mut count = 0u64;
    let mut full = 0.0f64;
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let l =
                0.2126 * rgba[i] as f64 + 0.7152 * rgba[i + 1] as f64 + 0.0722 * rgba[i + 2] as f64;
            full += l;
            if y >= top && y < bottom && x >= left && x < right {
                sum += l;
                count += 1;
            }
        }
    }
    (
        sum / count.max(1) as f64,
        full / (width as f64 * height as f64),
    )
}

fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

#[test]
fn daynight_frames_dawn_noon_midnight() {
    let Some(mut game) = game() else {
        eprintln!("SKIP daynight frames: War3x.mpq / map not present");
        return;
    };
    let scene = game.scene();
    let mut lum = Vec::new();
    for (phase, hour) in [
        // Middle of the DNC dawn ramp (05:20 -> 06:00).
        ("dawn", 5.5f32),
        ("noon", 12.0),
        ("midnight", 0.0),
        // Literal 06:00 frame: the DNC key is already full day.
        ("dawn0600", 6.0),
    ] {
        let rgba = match render::render_scene_to_rgba_at_hour(
            &game.assets,
            &game.font.atlas,
            game.assets.minimap.as_ref(),
            &scene,
            WIDTH,
            HEIGHT,
            hour,
        ) {
            Ok(rgba) => rgba,
            Err(error) => {
                eprintln!("SKIP daynight frames: headless render unavailable: {error}");
                return;
            }
        };
        let dir = repo().join(OUT_DIR);
        std::fs::create_dir_all(&dir).expect("create hudskin dir");
        let path = dir.join(format!("daynight-{phase}-{WIDTH}x{HEIGHT}.png"));
        write_png(&path, &rgba, WIDTH, HEIGHT);
        let (world, full) = world_luminance(&rgba, WIDTH, HEIGHT);
        println!(
            "DAYNIGHT phase={phase} hour={hour:.2} world_mean={world:.3} full_mean={full:.3} path={}",
            path.display()
        );
        lum.push((phase, world, full));
    }
    let noon = lum.iter().find(|(p, _, _)| *p == "noon").unwrap().1;
    let dawn = lum.iter().find(|(p, _, _)| *p == "dawn").unwrap().1;
    let midnight = lum.iter().find(|(p, _, _)| *p == "midnight").unwrap().1;
    assert!(
        noon > dawn && dawn > midnight,
        "day/night must order noon > dawn > midnight: {lum:?}"
    );
    assert!(
        midnight > 3.0,
        "midnight must not be pure black: {midnight}"
    );
    assert!(
        noon - midnight > 15.0,
        "night must be visibly darker than noon: noon={noon} midnight={midnight}"
    );
}
