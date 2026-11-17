//! LEDGER 476 lighting evidence frames (2414x1080).
//!
//! One generator for the four mission hours plus the dusk/night sample the
//! operator complains about. `H2_FRAME_TAG` selects the file prefix so the
//! "before" (current build) and "after" (fixed lighting) frames share one
//! camera, one map and one binary:
//!
//! ```text
//! H2_FRAME_TAG=before scripts/h2-cargo test -p wc3-render --test lighting_frames_476 -- --nocapture
//! ```
//!
//! Each frame prints its per-channel means in the world band and over the full
//! frame, which is what `analysis/agent-re/render/LIGHTING-476.md` records.

use std::path::PathBuf;

use wc3_game::{Assets, Game, render};

const WIDTH: u32 = 2414;
const HEIGHT: u32 = 1080;

/// Mean R/G/B over the world band (excludes the HUD, which WC3 keeps ungraded)
/// and over the full frame.
fn channel_means(rgba: &[u8], width: u32, height: u32) -> ([f64; 3], [f64; 3]) {
    let top = (height as f64 * 0.08) as u32;
    let bottom = (height as f64 * 0.70) as u32;
    let left = (width as f64 * 0.06) as u32;
    let right = (width as f64 * 0.94) as u32;
    let mut band = [0.0f64; 3];
    let mut count = 0u64;
    let mut full = [0.0f64; 3];
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            for channel in 0..3 {
                let value = rgba[i + channel] as f64;
                full[channel] += value;
                if y >= top && y < bottom && x >= left && x < right {
                    band[channel] += value;
                }
            }
            if y >= top && y < bottom && x >= left && x < right {
                count += 1;
            }
        }
    }
    for channel in 0..3 {
        band[channel] /= count.max(1) as f64;
    }
    let pixels = (width as f64 * height as f64).max(1.0);
    for channel in 0..3 {
        full[channel] /= pixels;
    }
    (band, full)
}

fn write_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

// The install/map discovery is the same one the day/night frames use, so the
// before/after pair is the same camera and the same assets.
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
    if let Some(path) = std::env::var_os("H2_WC3_MAP") {
        return Some(PathBuf::from(path));
    }
    [
        root.join("analysis/war3_map/Maps/Campaign/Human04.w3m"),
        root.join("donors/maps/melee/(2)Justice.w3x"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn game() -> Option<Game> {
    let mpq = install_mpq()?;
    let map = map_path()?;
    let assets = Assets::load(Some(&mpq), Some(map.as_path()));
    let mut game = Game::new(assets, 0x5EED);
    game.enter_battle();
    game.resize(WIDTH as f32, HEIGHT as f32);
    for _ in 0..30 {
        game.update(1.0 / 30.0);
    }
    Some(game)
}

#[test]
fn lighting_frames_476() {
    let Some(mut game) = game() else {
        eprintln!("SKIP lighting frames: War3x.mpq / map not present");
        return;
    };
    let tag = std::env::var("H2_FRAME_TAG").unwrap_or_else(|_| "before".to_string());
    let scene = game.scene();
    let out = repo().join("analysis/agent-re/render");
    std::fs::create_dir_all(&out).expect("create render dir");
    for (label, hour) in [
        ("0600", 6.0f32),
        ("0900", 9.0),
        ("1200", 12.0),
        ("1800", 18.0),
        ("1930", 19.5),
        ("2100", 21.0),
        ("0000", 0.0),
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
                eprintln!("SKIP lighting frames: headless render unavailable: {error}");
                return;
            }
        };
        let path = out.join(format!("light-{label}-{tag}-{WIDTH}x{HEIGHT}.png"));
        write_png(&path, &rgba, WIDTH, HEIGHT);
        let (band, full) = channel_means(&rgba, WIDTH, HEIGHT);
        println!(
            "LIGHTING476 tag={tag} hour={hour:05.2} band_r={:.2} band_g={:.2} band_b={:.2} \
             band_dRG={:+.2} band_dGB={:+.2} full_r={:.2} full_g={:.2} full_b={:.2} path={}",
            band[0],
            band[1],
            band[2],
            band[0] - band[1],
            band[1] - band[2],
            full[0],
            full[1],
            full[2],
            path.display()
        );
    }
}
