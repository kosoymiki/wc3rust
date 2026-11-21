//! Audit probe (lane J, ledger 530; fixed under ledger 540): fog-mask contract
//! and cost.
//!
//! One-shot evidence for TZ §3.3, FMT-NEW-006 and WC3-RENDER-001/002/003. Not a
//! test: every case is a separate process invocation so the exact exit status is
//! part of the evidence (under `panic = "abort"` a bug used to show up as 134).
//!
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- stride
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- rebuild
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- cost
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- sky
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- mismatch
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- overflow
//!     scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- zero
//!
//!  1. `stride`   – `fog_mask_from_sim` geometry: stride, cell count, <=256 per
//!                  side for every grid, block coverage and world->cell mapping.
//!  2. `rebuild`  – the change gate: N identical frames must upload once.
//!  3. `cost`     – CPU cost of the per-frame mask path (128², 256², 4096²).
//!  4. `sky`      – clear-depth pixels cannot become world fragments
//!                  (WC3-I-013; no GPU needed — the shader's decision is a
//!                  pure function of depth + inverse view-projection).
//!  5. `mismatch` – `FogMask::try_new` rejects, `new` repairs, no abort.
//!  6. `overflow` – a 65536² mask is refused/repaired, `state_at` saturates.
//!  7. `zero`     – `fog_mask_from_sim` on a zero-sized grid.

use std::time::Instant;

use wc3_render::fog::{darkness_grid, FogMask, FogMaskCache};
use wc3_sim::{Sim, UnitKind, Vec2};

fn timed<T>(label: &str, body: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = body();
    println!("{label}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    value
}

fn sim_fog(width: u32, height: u32) {
    let sim = Sim::new(0, width, height);
    let mask = wc3_game::render::fog_mask_from_sim(&sim, 0);
    // Stride printed from the same inputs the builder uses (ceiling division).
    let stride = width.max(height).max(1).div_ceil(256).max(1);
    let side_ok = mask.width() <= 256 && mask.height() <= 256;
    let cells_ok = mask.cell_count() as u64 <= 256 * 256;
    // Coverage: every grid cell is inside some mask cell, and the last mask
    // cell overshoots the grid by less than one stride (no gap, no extra block).
    let cover_x = mask.width() as u64 * stride as u64;
    let cover_y = mask.height() as u64 * stride as u64;
    let cover_ok = cover_x >= width.max(1) as u64
        && cover_y >= height.max(1) as u64
        && cover_x - width.max(1) as u64 <= stride as u64
        && cover_y - height.max(1) as u64 <= stride as u64;
    // Mapping (WC3-I-012): the CPU now samples each mask cell's **block
    // centre** `(x + 0.5) * stride * cell_size`, and the shader's
    // `floor(world / (cell_size * stride))` must return that same cell for
    // every cell (the partial last block is the only edge case, and the CPU
    // clamps its sample to the last real pathing cell).
    let published_cell = mask.cell_size().max(1.0);
    let mut mapping_ok = true;
    let mut mapping_checked = 0u32;
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            let centre_x = (x as f32 + 0.5) * stride as f32 * sim.grid.cell_size;
            let centre_y = (y as f32 + 0.5) * stride as f32 * sim.grid.cell_size;
            let shader_x = (centre_x / published_cell).floor() as u32;
            let shader_y = (centre_y / published_cell).floor() as u32;
            mapping_ok &= shader_x == x && shader_y == y;
            mapping_checked += 1;
        }
    }
    println!(
        "grid={}x{} stride={} -> fog={}x{} cells={} cell_size={:.1} (world span {:.0}) \
         cells<=256={} side<=256={} cover={} aligned={} (centre, {} cells)",
        width,
        height,
        stride,
        mask.width(),
        mask.height(),
        mask.cell_count(),
        mask.cell_size(),
        mask.width() as f32 * mask.cell_size(),
        cells_ok,
        side_ok,
        cover_ok,
        mapping_ok,
        mapping_checked,
    );
}

fn rebuild(width: u32, height: u32, frames: u32) {
    let mut sim = Sim::new(0, width, height);
    let mut cache = FogMaskCache::new();
    let mut changed = 0u32;
    let build = timed(&format!("fog_mask_from_sim({width}x{height}) x{frames}"), || {
        let mut total = std::time::Duration::ZERO;
        for _ in 0..frames {
            let start = Instant::now();
            let mask = wc3_game::render::fog_mask_from_sim(&sim, 0);
            total += start.elapsed();
            if cache.accept(&mask) {
                changed += 1;
            }
        }
        total
    });
    let per_frame_us = build.as_secs_f64() * 1e6 / frames as f64;
    println!(
        "grid={width}x{height} frames={frames} mask_rebuilds={changed} cache_rebuilds={} \
         unchanged={} per_frame_sampling={per_frame_us:.0}us",
        cache.rebuilds(),
        frames - changed,
    );
    // One visibility change must produce exactly one more rebuild.
    let before = cache.rebuilds();
    sim.spawn(0, UnitKind::Footman, Vec2::new(1600.0, 1600.0));
    sim.step(&[]);
    let mask = wc3_game::render::fog_mask_from_sim(&sim, 0);
    let accepted = cache.accept(&mask);
    println!(
        "change_event=spawn+step accepted={accepted} rebuilds_before={before} \
         rebuilds_after={} fog_cells={}",
        cache.rebuilds(),
        mask.cell_count(),
    );
    // And the blur/upload half of the path the gate protects, at this size.
    let blurred = timed(&format!("darkness_grid({}x{})", mask.width(), mask.height()), || {
        darkness_grid(&mask)
    });
    println!(
        "upload_bytes_at_this_size={} blurred_cells={}",
        mask.cell_count(),
        blurred.len()
    );
}

/// Clear-depth pixels cannot be mapped to a world fragment.
///
/// The fog pass reconstructs `world = inv_view_proj * vec4(ndc, depth, 1)`. The
/// world pass clears depth to exactly 1.0 and colour to the atmospheric colour,
/// and the fog pass loads rather than clears; a pixel the veil does not map must
/// therefore be *left alone*. This case evaluates the shader's decision for
/// clear-depth pixels on a normal perspective camera and shows they all take the
/// `depth >= 0.9999999` branch (WC3-I-013), so the previous
/// `vec4(0,0,0,1)` fallback — opaque black over the cleared atmosphere — was the
/// only thing painting the sky black.
fn sky() {
    use glam::{Mat4, Vec2, Vec3, Vec4};
    let view_proj = Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 1.0, 100_000.0)
        * Mat4::look_at_rh(Vec3::new(0.0, 1200.0, -4000.0), Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
    let inv = view_proj.inverse();
    println!("== fog pass background handling (WC3-I-013) ==");
    for depth in [1.0f32, 0.999_999_9, 0.999, 0.5] {
        let (mut clear_branch, mut mapped) = (0u32, 0u32);
        for gy in 0..=8 {
            for gx in 0..=8 {
                let ndc = Vec2::new(gx as f32 / 8.0 * 2.0 - 1.0, gy as f32 / 8.0 * 2.0 - 1.0);
                let world4: Vec4 = inv * Vec4::new(ndc.x, ndc.y, depth, 1.0);
                // Branch order of `fs_main`: clear depth first, then w <= 0.
                if depth >= 0.999_999_9 || !world4.is_finite() || world4.w <= 0.0 {
                    clear_branch += 1;
                } else {
                    mapped += 1;
                }
            }
        }
        println!(
            "depth={depth:.7} samples=81 discard_no_world={clear_branch} mapped={mapped}"
        );
    }
    println!(
        "shader branches: depth>=0.9999999 -> discard; world4.w<=0 -> discard; \n         outside [0,grid_w)x[0,grid_h) -> discard; opaque black only for state 0 inside the grid"
    );
}

fn main() {
    let case = std::env::args().nth(1).unwrap_or_else(|| "stride".to_string());
    match case.as_str() {
        "stride" => {
            println!("== fog_mask_from_sim geometry (WC3-RENDER-001) ==");
            // 128 = a 128x128-tile map (what `Game::new` builds: `map_width_cells`).
            for (w, h) in [
                (128u32, 128u32),
                (256, 256),
                (4000, 4000),
                (511, 511),
                (4096, 4096),
                (257, 257),
                (1, 1),
            ] {
                sim_fog(w, h);
            }
        }
        "rebuild" => {
            println!("== fog change gate (WC3-RENDER-002) ==");
            for (w, h) in [(128u32, 128u32), (256, 256), (4096, 4096)] {
                rebuild(w, h, 120);
            }
        }
        "cost" => {
            println!("== per-frame CPU cost of the fog path ==");
            for (w, h) in [(128u32, 128u32), (256, 256)] {
                let sim = Sim::new(0, w, h);
                let mask = timed(&format!("fog_mask_from_sim({w}x{h})"), || {
                    wc3_game::render::fog_mask_from_sim(&sim, 0)
                });
                let grid = timed(
                    &format!("darkness_grid({}x{})", mask.width(), mask.height()),
                    || darkness_grid(&mask),
                );
                println!(
                    "  cells={} blurred_bytes={}",
                    mask.cell_count(),
                    grid.len() * 4
                );
            }
            // The un-capped path the old code could reach: the gate exists so
            // this never runs per frame.
            let full = FogMask::new(4096, 4096, 4.0, vec![2u8; 4096 * 4096]);
            let grid = timed("darkness_grid(4096x4096 = 16.7M cells, 5x5 blur)", || {
                darkness_grid(&full)
            });
            println!(
                "  blurred cells={} f32 bytes={}",
                grid.len(),
                grid.len() * 4
            );
            let sim = Sim::new(0, 4096, 4096);
            let mask = timed("fog_mask_from_sim(4096x4096 pathing grid)", || {
                wc3_game::render::fog_mask_from_sim(&sim, 0)
            });
            println!(
                "  fog={}x{} cells={} cell_size={:.1}",
                mask.width(),
                mask.height(),
                mask.cell_count(),
                mask.cell_size()
            );
        }
        "sky" => sky(),
        "mismatch" => {
            println!("== FogMask release validation (WC3-RENDER-003) ==");
            let rejected = FogMask::try_new(4, 4, 128.0, vec![2, 2, 2]);
            println!("try_new(4,4,len=3) -> {rejected:?}");
            let repaired = FogMask::new(4, 4, 128.0, vec![2, 2, 2]);
            println!(
                "new(4,4,len=3) -> len={} (repaired; padded cells read {} = unexplored)",
                repaired.cell_count(),
                repaired.state_at(3, 3),
            );
            let zeroed = darkness_grid(&repaired);
            let padded_dark = zeroed.iter().skip(3).all(|value| *value >= 0.99);
            println!(
                "darkness_grid: len={} padded_cells_dark={} worst={:.3} — missing cells fail dark, never 'visible'",
                zeroed.len(),
                padded_dark,
                zeroed.iter().cloned().fold(0.0f32, f32::max),
            );
            println!("state_at(3,3)={} (no abort)", repaired.state_at(3, 3));
        }
        "overflow" => {
            println!("== state_at index arithmetic (WC3-RENDER-003) ==");
            let rejected = FogMask::try_new(65_536, 65_536, 128.0, Vec::new());
            println!("try_new(65536,65536,cells=0) -> {rejected:?}");
            let repaired = FogMask::new(65_536, 65_536, 128.0, Vec::new());
            println!(
                "new(65536,65536,cells=0) -> {}x{} len={} (4 GiB repair refused)",
                repaired.width(),
                repaired.height(),
                repaired.cell_count(),
            );
            println!(
                "state_at(1,65535)={} (saturates, no abort)",
                repaired.state_at(1, 65_535)
            );
        }
        "zero" => {
            println!("== zero-sized grid ==");
            sim_fog(0, 0);
        }
        other => println!("unknown case {other}"),
    }
}
