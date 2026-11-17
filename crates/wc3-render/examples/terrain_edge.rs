//! Headless Justice terrain frame for operator row 14 (water/shoreline/cliff).
//!
//! Renders the device frame (2414x1080) from the Justice start view, from the
//! nearest sea-level map edge and from the nearest shoreline + cliff cell, and
//! writes PNGs under `analysis/agent-qa/terrain/` with the `JUSTICE_EDGE`
//! metrics (flat clear-colour band, water coverage).
//!
//! `H2_TERRAIN_TAG=before|after` names the artifacts. Needs War3.mpq + GPU.
//!
//! Run: `scripts/h2-cargo run -p wc3-render --offline --example terrain_edge`

use std::path::PathBuf;

use glam::Vec2;
use wc3_game::{Assets, Game};

const TERRAIN_TILE_SIZE: f32 = 128.0;

fn main() {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
    .expect("War3.mpq");
    let map = PathBuf::from(
        "/data/data/com.termux/files/home/src/wc3rust/donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    let tag = std::env::var("H2_TERRAIN_TAG").unwrap_or_else(|_| "after".to_string());
    let mut game = Game::new(Assets::load(Some(&mpq), Some(&map)), 0x5EED);
    game.update(wc3_game::game::TICK_SECONDS);
    let width = 2414u32;
    let height = 1080u32;
    game.camera.viewport = Vec2::new(width as f32, height as f32);
    let start = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().is_main_hall())
        .map(|unit| unit.pos)
        .unwrap_or(game.camera.target);

    let assets = &game.assets;
    let cells_w = assets.map_width_cells as usize;
    let cells_h = assets.map_height_cells as usize;
    let stride = cells_w + 1;
    let corner = |x: usize, z: usize| assets.terrain_tiles.get(z * stride + x);
    // Max final height around a cell: the RTS camera eye sits `0.92 * distance`
    // above y = 0, so a close-up of high ground needs a taller camera.
    let relief = |cx: usize, cz: usize| -> f32 {
        let mut high = 0.0f32;
        for dz in 0..=4usize {
            for dx in 0..=4usize {
                if let Some(tile) = corner(cx + dx, cz + dz) {
                    let h = (tile.height() + (tile.cliff_level() as f32 - 2.0)) * 128.0;
                    high = high.max(h);
                }
            }
        }
        high
    };

    // Water next to a land cliff within two tiles.
    let mut water_cliff: Option<(usize, usize, f32, f32, u32)> = None;
    for z in 6..cells_h.saturating_sub(6) {
        for x in 6..cells_w.saturating_sub(6) {
            let Some(a) = corner(x, z) else { continue };
            if !a.is_water() {
                continue;
            }
            let mut step = 0u8;
            for dz in 0..=2usize {
                for dx in 0..=2usize {
                    if let Some(b) = corner(x + dx, z + dz) {
                        if !b.is_water() {
                            step = step.max(b.cliff_level().saturating_sub(a.cliff_level()));
                        }
                    }
                }
            }
            if step < 2 {
                continue;
            }
            let distance = ((x as f32 - start.x / TERRAIN_TILE_SIZE).powi(2)
                + (z as f32 - start.y / TERRAIN_TILE_SIZE).powi(2))
            .sqrt();
            // Prefer the most water in frame: a lone pond corner is not a
            // readable water+cliff shot.
            let mut wet = 0u32;
            for dz in -2i32..=2 {
                for dx in -2i32..=2 {
                    let nx = (x as i32 + dx).clamp(0, cells_w as i32 - 1) as usize;
                    let nz = (z as i32 + dz).clamp(0, cells_h as i32 - 1) as usize;
                    if corner(nx, nz).map(|tile| tile.is_water()).unwrap_or(false) {
                        wet += 1;
                    }
                }
            }
            let high = relief(x, z);
            if water_cliff
                .map(|(_, _, best_distance, _, best_wet)| {
                    wet > best_wet || (wet == best_wet && distance < best_distance)
                })
                .unwrap_or(true)
            {
                water_cliff = Some((x, z, distance, high, wet));
            }
        }
    }

    // W3E water flag at the map boundary (one tile inside the frame).
    let mut map_edge: Option<(usize, usize, f32)> = None;
    for z in 0..cells_h {
        for x in 0..cells_w {
            let on_edge = x <= 1 || z <= 1 || x + 2 >= cells_w || z + 2 >= cells_h;
            if !on_edge {
                continue;
            }
            let Some(tile) = corner(x.max(1).min(cells_w - 1), z.max(1).min(cells_h - 1)) else {
                continue;
            };
            if !tile.is_water() {
                continue;
            }
            let distance = ((x as f32 - start.x / TERRAIN_TILE_SIZE).powi(2)
                + (z as f32 - start.y / TERRAIN_TILE_SIZE).powi(2))
            .sqrt();
            if map_edge.map(|(_, _, best)| distance < best).unwrap_or(true) {
                map_edge = Some((x, z, distance));
            }
        }
    }

    let dir = PathBuf::from("/data/data/com.termux/files/home/src/wc3rust/analysis/agent-qa/terrain");
    std::fs::create_dir_all(&dir).expect("terrain dir");
    let options = wc3_game::render::RenderOptions {
        skip_ui: true,
        ..Default::default()
    };
    let font = wc3_game::hud::Font::load();
    let mut shots: Vec<(String, Vec2, f32)> = vec![("edge".to_string(), start, 1900.0)];
    if let Some((x, z, _)) = map_edge {
        shots.push((
            "map-edge".to_string(),
            Vec2::new(
                (x as f32 + 0.5) * TERRAIN_TILE_SIZE,
                (z as f32 + 0.5) * TERRAIN_TILE_SIZE,
            ),
            1500.0,
        ));
    }
    if let Some((x, z, _, high, _)) = water_cliff {
        let target_x = ((x as f32 + 2.0) * TERRAIN_TILE_SIZE)
            .min((cells_w as f32 - 0.5) * TERRAIN_TILE_SIZE);
        let target_z = ((z as f32 + 2.0) * TERRAIN_TILE_SIZE)
            .min((cells_h as f32 - 0.5) * TERRAIN_TILE_SIZE);
        // 0.92 * distance is the eye height; keep it above the local relief.
        let distance = ((high + 350.0) / 0.92).max(1400.0);
        shots.push((
            "water-cliff".to_string(),
            Vec2::new(target_x, target_z),
            distance,
        ));
    }
    for (label, target, distance) in shots {
        game.camera.target = target;
        game.camera.distance = distance;
        let scene = game.scene();
        let rgba = wc3_game::render::render_scene_to_rgba_with(
            &game.assets,
            &font.atlas,
            None,
            &scene,
            width,
            height,
            options,
        )
        .expect("justice edge render");
        let path = dir.join(format!("{tag}-justice-{label}-{width}x{height}.png"));
        wc3_game::render::write_png(path.to_str().unwrap(), width, height, &rgba)
            .expect("write png");
        let clear = [60i32, 77, 93];
        let mut flat = 0u64;
        let mut water = 0u64;
        let mut total = 0u64;
        for pixel in rgba.chunks_exact(4) {
            let rgb = [pixel[0] as i32, pixel[1] as i32, pixel[2] as i32];
            total += 1;
            if (rgb[0] - clear[0]).abs() <= 2
                && (rgb[1] - clear[1]).abs() <= 2
                && (rgb[2] - clear[2]).abs() <= 2
            {
                flat += 1;
            }
            if rgb[2] > rgb[0] + 12 && rgb[2] > rgb[1] + 2 && rgb[2] > 40 {
                water += 1;
            }
        }
        println!(
            "JUSTICE_EDGE tag={tag} shot={label} target=({:.0},{:.0}) distance={distance:.0} flat_clear={:.4} water={:.4} file={}",
            target.x,
            target.y,
            flat as f64 / total.max(1) as f64,
            water as f64 / total.max(1) as f64,
            path.display()
        );
    }
}
