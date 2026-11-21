//! art10_ghost_visible — bug #10 evidence (Agent B, ledger 586 / resumed 598).
//!
//! Operator report #10 (REPORT.md:20): "hold frame shows no visible translucent
//! building (region diff 30.8 from units/camera)". The draw path at
//! game.rs:7265 already emits a 50%-alpha real MDX, green/red tint and a
//! footprint ring; this file pins that the *shipped scene* really contains all
//! three, and — with the extracted install mounted — that the shipped renderer
//! produces a frame where the preview is visibly different from the same camera
//! without it (the operator's own "region diff" metric).
//!
//! - `art10_ghost_visible`: scene-level contract on the fallback assets.
//! - `art10_ghost_visible_render`: real assets + headless Vulkan frame, PNG/PPM
//!   artefacts under `analysis/agent-hud/all-layers-20261121/` (headless, NOT
//!   device: the device slot is agent G's).

use std::path::{Path, PathBuf};

use wc3_game::{Assets, Game};
use wc3_sim::Vec2;

fn ghost_def() -> &'static str {
    for candidate in ["hgtw", "otot", "hhou", "ogre"] {
        if let Some(def) = wc3_sim::def_of(candidate)
            && def.stats.is_building
        {
            return def.id;
        }
    }
    "hhou"
}

#[test]
fn art10_ghost_visible() {
    let mut game = Game::new(Assets::load(None, None), 0xA10);
    game.enter_battle();
    game.resize(2414.0, 1080.0);
    for _ in 0..5 {
        game.update(1.0 / 30.0);
    }

    let def_id = ghost_def();
    game.begin_placement(def_id);
    let ghost = game.ghost().expect("a placement has a live ghost");
    assert_eq!(ghost.def_id, def_id, "ghost carries the chosen building def");
    let at = ghost.at;
    let model = game.assets.model_index_for_def(def_id);
    let legal = ghost.legal;

    let scene = game.scene();
    // The ghost is appended after every live unit; take the last model match.
    let draw = scene
        .units
        .iter()
        .rev()
        .find(|unit| {
            unit.model == model
                && (unit.world.w_axis.x - at.x).abs() < 1.0
                && (unit.world.w_axis.z - at.y).abs() < 1.0
        })
        .expect("ghost model is drawn in the scene at the pointer");

    // Translucent: every alpha the model supplies is scaled by 0.5.
    let alpha = draw
        .mesh_alpha
        .iter()
        .copied()
        .chain(draw.layer_alpha.iter().flat_map(|l| l.iter().copied()))
        .fold(0.0f32, f32::max);
    if alpha > 0.0 {
        assert!(
            (alpha - 0.5).abs() < 0.06,
            "ghost must be 50% alpha, measured {alpha}"
        );
    } // A placeholder model with no geometry has no alpha track; tint+ring still prove the draw.

    // Green when valid, red when the footprint is blocked.
    if legal {
        assert!(
            draw.tint[1] > draw.tint[0] + 0.2,
            "legal ghost is green: {:?}",
            draw.tint
        );
    } else {
        assert!(
            draw.tint[0] > draw.tint[1] + 0.2,
            "blocked ghost is red: {:?}",
            draw.tint
        );
    }

    // Footprint ring: radius derived from the authored footprint, coloured to
    // match validity. This is the marker the operator could not see.
    let ring = scene
        .rings
        .iter()
        .rev()
        .find(|ring| {
            (ring.pos.x - at.x).abs() < 1.0 && (ring.pos.z - at.y).abs() < 1.0 && ring.radius > 40.0
        })
        .expect("a footprint ring is drawn under the ghost");
    if legal {
        assert!(ring.color[1] > ring.color[0], "legal ring is green");
    } else {
        assert!(ring.color[0] > ring.color[1], "blocked ring is red");
    }

    // Flip validity through the public pointer path: sweep the visible screen
    // until the strict placement predicate reports a blocked site.
    let mut forbidden = None;
    for gy in 0..10u32 {
        for gx in 0..16u32 {
            let screen = Vec2::new(
                120.0 + gx as f32 * (2414.0 - 240.0) / 15.0,
                180.0 + gy as f32 * (760.0 - 180.0) / 9.0,
            );
            game.pointer_move(screen);
            if let Some(g) = game.ghost()
                && !g.legal
            {
                forbidden = Some(g.at);
                break;
            }
        }
        if forbidden.is_some() {
            break;
        }
    }
    if let Some(blocked_at) = forbidden {
        let scene = game.scene();
        let bad = scene
            .units
            .iter()
            .rev()
            .find(|unit| {
                unit.model == model
                    && (unit.world.w_axis.x - blocked_at.x).abs() < 1.0
                    && (unit.world.w_axis.z - blocked_at.y).abs() < 1.0
            })
            .expect("blocked ghost is drawn");
        assert!(
            bad.tint[0] > bad.tint[1],
            "blocked ghost tint must flip to red: {:?}",
            bad.tint
        );
        eprintln!("ART10_GHOST_INVALID at=({:.0},{:.0}) red=PASS", blocked_at.x, blocked_at.y);
    } else {
        eprintln!("ART10_GHOST_INVALID no blocked screen cell in the swept band (legal-only map)");
    }

    eprintln!(
        "ART10_GHOST_VISIBLE def={} at=({:.0},{:.0}) legal={} alpha={:.2} ring_r={:.0} PASS",
        def_id, at.x, at.y, legal, alpha, ring.radius
    );
}

// ---------------------------------------------------------------------------
// Shipped-asset proof: the same ghost contract with real MDX geometry.
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn shipped_map() -> Option<PathBuf> {
    let map = repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m");
    map.is_file().then_some(map)
}

/// The same source order the visual regression uses: the operator's MPQ first,
/// the extracted install second. `None` means no shipped data on this machine.
fn shipped_assets(map: &Path) -> Option<(Assets, String)> {
    for mpq in [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ] {
        let mpq = Path::new(mpq);
        if mpq.is_file() {
            return Some((
                Assets::load(Some(mpq), Some(map)),
                format!("mpq={}", mpq.display()),
            ));
        }
    }
    let loose = repo_root().join("analysis/war3_full3");
    loose.is_dir().then(|| {
        (
            Assets::load(Some(&loose), Some(map)),
            format!("loose={}", loose.display()),
        )
    })
}

/// The operator's ghost frame, with the real install mounted: a real MDX at
/// half alpha, the authored footprint ring and a red flip on a blocked cell.
#[test]
fn art10_ghost_visible_shipped() {
    let Some(map) = shipped_map() else {
        eprintln!("ART10_SHIPPED_SKIP analysis/war3_map/Maps/Campaign/Human04.w3m missing");
        return;
    };
    let Some((assets, source)) = shipped_assets(&map) else {
        eprintln!("ART10_SHIPPED_SKIP no War3.mpq / analysis/war3_full3 on this machine");
        return;
    };
    if !assets.models.iter().any(|model| !model.fallback) {
        eprintln!("ART10_SHIPPED_SKIP shipped assets loaded without real MDX models");
        return;
    }
    let mut game = Game::new(assets, 0xB10);
    game.enter_battle();
    game.resize(2414.0, 1080.0);
    for _ in 0..8 {
        game.update(1.0 / 30.0);
    }

    let anchor = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.is_building())
        .map(|unit| unit.pos)
        .expect("the map boots with a player building");
    let center = game.sim.grid.world_to_cell(anchor);
    let mut legal = None;
    let mut blocked = None;
    let mut chosen = None;
    'defs: for def_id in ["hhou", "hgtw", "hbar", "ogre", "otot"] {
        let Some(def) = wc3_sim::def_of(def_id) else {
            continue;
        };
        if !def.stats.is_building {
            continue;
        }
        game.begin_placement(def_id);
        for dz in -12..=12i32 {
            for dx in -12..=12i32 {
                let gx = center.0 as i32 + dx;
                let gy = center.1 as i32 + dz;
                if gx < 0
                    || gy < 0
                    || gx >= game.sim.grid.width as i32
                    || gy >= game.sim.grid.height as i32
                {
                    continue;
                }
                let at = game.sim.grid.cell_to_world(gx as u32, gy as u32);
                game.placing_at = Some(at);
                if let Some(ghost) = game.ghost() {
                    if ghost.legal && legal.is_none() {
                        legal = Some(at);
                    } else if !ghost.legal && blocked.is_none() {
                        blocked = Some(at);
                    }
                }
            }
        }
        game.placing = None;
        game.placing_at = None;
        if let Some(at) = legal {
            chosen = Some((def_id, at));
            break 'defs;
        }
    }
    let Some((def_id, legal_at)) = chosen else {
        eprintln!("ART10_SHIPPED_SKIP no legal footprint near the base");
        return;
    };
    let model = game.assets.model_index_for_def(def_id);
    assert!(
        !game.assets.model_for_def(def_id).fallback,
        "{def_id} must resolve to a real shipped MDX"
    );

    game.begin_placement(def_id);
    game.placing_at = Some(legal_at);
    let scene = game.scene();
    let draw = scene
        .units
        .iter()
        .rev()
        .find(|unit| {
            unit.model == model
                && (unit.world.w_axis.x - legal_at.x).abs() < 1.0
                && (unit.world.w_axis.z - legal_at.y).abs() < 1.0
        })
        .expect("the real ghost model is in the scene");
    let alpha = draw
        .mesh_alpha
        .iter()
        .copied()
        .chain(draw.layer_alpha.iter().flat_map(|layers| layers.iter().copied()))
        .fold(0.0f32, f32::max);
    assert!(
        !draw.mesh_alpha.is_empty() || draw.layer_alpha.iter().any(|layers| !layers.is_empty()),
        "a real MDX ghost carries an alpha track (visible geometry), model={model}"
    );
    assert!(
        (alpha - 0.5).abs() < 0.01,
        "real MDX ghost must be exactly 50% alpha, measured {alpha}"
    );
    assert!(
        draw.tint[1] > draw.tint[0] + 0.2,
        "legal real ghost is green: {:?}",
        draw.tint
    );
    let ring = scene
        .rings
        .iter()
        .rev()
        .find(|ring| {
            (ring.pos.x - legal_at.x).abs() < 1.0
                && (ring.pos.z - legal_at.y).abs() < 1.0
                && ring.radius > 40.0
        })
        .expect("the real footprint ring is in the scene");
    let footprint = wc3_sim::def_of(def_id).expect("def").footprint.max(1) as f32;
    let expected = footprint * 128.0 * 0.5 + 24.0;
    assert!(
        (ring.radius - expected).abs() < 0.5,
        "ring radius must follow the authored footprint: {ring_radius} vs {expected}",
        ring_radius = ring.radius
    );
    assert!(ring.color[1] > ring.color[0], "legal ring is green");

    if let Some(blocked_at) = blocked {
        game.placing_at = Some(blocked_at);
        if game.ghost().is_some_and(|ghost| !ghost.legal) {
            let scene = game.scene();
            let bad = scene
                .units
                .iter()
                .rev()
                .find(|unit| {
                    unit.model == model
                        && (unit.world.w_axis.x - blocked_at.x).abs() < 1.0
                        && (unit.world.w_axis.z - blocked_at.y).abs() < 1.0
                })
                .expect("the blocked ghost is still drawn");
            assert!(
                bad.tint[0] > bad.tint[1] + 0.2,
                "blocked real ghost is red: {:?}",
                bad.tint
            );
            let bad_ring = scene
                .rings
                .iter()
                .rev()
                .find(|ring| {
                    (ring.pos.x - blocked_at.x).abs() < 1.0
                        && (ring.pos.z - blocked_at.y).abs() < 1.0
                        && ring.radius > 40.0
                })
                .expect("the blocked ring is still drawn");
            assert!(bad_ring.color[0] > bad_ring.color[1], "blocked ring is red");
            eprintln!(
                "ART10_SHIPPED_BLOCKED at=({:.0},{:.0}) red=PASS",
                blocked_at.x, blocked_at.y
            );
        }
    }

    eprintln!(
        "ART10_SHIPPED_VISIBLE source={source} def={def_id} at=({:.0},{:.0}) alpha={alpha:.3} \
         ring_r={:.1} footprint={footprint} PASS",
        legal_at.x, legal_at.y, ring.radius
    );
}

/// Screen-space box of the ghost: footprint corners, model top and ring radius
/// projected with the same camera the renderer uses.
fn ghost_region(game: &Game, def_id: &str, at: Vec2) -> Option<(u32, u32, u32, u32)> {
    let model = game.assets.model_for_def(def_id);
    let ground = game.assets.height_at(at.x, at.y);
    let footprint = wc3_sim::def_of(def_id)?.footprint.max(1) as f32;
    let half = footprint * 128.0 * 0.5;
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut add = |world| {
        if let Some(screen) = game.camera.world_to_screen(world) {
            min = min.min(screen);
            max = max.max(screen);
        }
    };
    for (dx, dz) in [
        (-half, -half),
        (half, -half),
        (-half, half),
        (half, half),
        (-half - 24.0, 0.0),
        (half + 24.0, 0.0),
        (0.0, -half - 24.0),
        (0.0, half + 24.0),
    ] {
        add(glam::Vec3::new(at.x + dx, ground + 4.0, at.y + dz));
    }
    add(glam::Vec3::new(
        at.x,
        ground + model.height.max(96.0),
        at.y,
    ));
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let viewport = game.viewport;
    let x0 = (min.x.floor() as i64 - 10).clamp(0, viewport.x as i64 - 2) as u32;
    let y0 = (min.y.floor() as i64 - 10).clamp(0, viewport.y as i64 - 2) as u32;
    let x1 = (max.x.ceil() as i64 + 10).clamp(x0 as i64 + 1, viewport.x as i64 - 1) as u32;
    let y1 = (max.y.ceil() as i64 + 10).clamp(y0 as i64 + 1, viewport.y as i64 - 1) as u32;
    if x1 <= x0 + 8 || y1 <= y0 + 8 {
        return None;
    }
    Some((x0, y0, x1, y1))
}

struct RegionStats {
    total: u64,
    changed: u64,
    red: u64,
}

/// The operator's region-diff metric (0..255 units) plus the red-hue count.
fn region_stats(a: &[u8], b: &[u8], width: u32, region: (u32, u32, u32, u32)) -> RegionStats {
    let (x0, y0, x1, y1) = region;
    let mut stats = RegionStats {
        total: 0,
        changed: 0,
        red: 0,
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let index = ((y * width + x) * 4) as usize;
            let (r, g, blue) = (a[index], a[index + 1], a[index + 2]);
            let (r2, g2, b2) = (b[index], b[index + 1], b[index + 2]);
            stats.total += 1;
            let delta = (r as i32 - r2 as i32)
                .abs()
                .max((g as i32 - g2 as i32).abs())
                .max((blue as i32 - b2 as i32).abs());
            if delta > 10 {
                stats.changed += 1;
            }
            let (r, g, blue) = (r as i32, g as i32, blue as i32);
            if r > 70 && r > g + 14 && r > blue + 14 {
                stats.red += 1;
            }
        }
    }
    stats
}

/// Optional headless-Vulkan frames for the #10 evidence bundle: the operator's
/// own "region diff" metric, measured on a real shipped scene. Off by default
/// (shared GPU, ~1-2 min/frame; device slot is agent G's).
#[test]
fn art10_ghost_visible_gpu_frame() {
    if std::env::var("H2_GPU_EVIDENCE").ok().as_deref() != Some("1") {
        eprintln!("ART10_GPU_SKIP set H2_GPU_EVIDENCE=1 to render headless frames");
        return;
    }
    let Some(map) = shipped_map() else {
        eprintln!("ART10_GPU_SKIP Human04.w3m missing");
        return;
    };
    let Some((assets, source)) = shipped_assets(&map) else {
        eprintln!("ART10_GPU_SKIP no shipped install");
        return;
    };
    let mut game = Game::new(assets, 0xB10);
    game.enter_battle();
    game.resize(2414.0, 1080.0);
    for _ in 0..8 {
        game.update(1.0 / 30.0);
    }
    let anchor = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.is_building())
        .map(|unit| unit.pos)
        .expect("player building");
    let center = game.sim.grid.world_to_cell(anchor);
    let mut legal = None;
    let mut blocked = None;
    let mut chosen = None;
    for def_id in ["hhou", "hgtw", "hbar"] {
        let Some(def) = wc3_sim::def_of(def_id) else {
            continue;
        };
        if !def.stats.is_building {
            continue;
        }
        game.begin_placement(def_id);
        for dz in -12..=12i32 {
            for dx in -12..=12i32 {
                let gx = center.0 as i32 + dx;
                let gy = center.1 as i32 + dz;
                if gx < 0
                    || gy < 0
                    || gx >= game.sim.grid.width as i32
                    || gy >= game.sim.grid.height as i32
                {
                    continue;
                }
                let at = game.sim.grid.cell_to_world(gx as u32, gy as u32);
                game.placing_at = Some(at);
                if let Some(ghost) = game.ghost() {
                    if ghost.legal && legal.is_none() {
                        legal = Some(at);
                    } else if !ghost.legal && blocked.is_none() {
                        blocked = Some(at);
                    }
                }
            }
        }
        game.placing = None;
        game.placing_at = None;
        if let Some(at) = legal {
            chosen = Some((def_id, at));
            break;
        }
    }
    let Some((def_id, legal_at)) = chosen else {
        eprintln!("ART10_GPU_SKIP no legal footprint");
        return;
    };
    let out_dir = repo_root().join("analysis/agent-hud/all-layers-20261121");
    std::fs::create_dir_all(&out_dir).expect("evidence dir");

    // Baseline frame: same camera, no ghost.
    game.camera.centre_on(legal_at);
    let scene = game.scene();
    let baseline = wc3_game::render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        2414,
        1080,
    )
    .expect("baseline frame");
    wc3_game::render::write_ppm(
        out_dir
            .join("headless-NOT-DEVICE-ghost-baseline-2414x1080.ppm")
            .to_str()
            .expect("utf8 path"),
        2414,
        1080,
        &baseline,
    );

    // Legal preview.
    game.begin_placement(def_id);
    game.placing_at = Some(legal_at);
    let region = ghost_region(&game, def_id, legal_at).expect("legal ghost box");
    let scene = game.scene();
    let legal = wc3_game::render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        2414,
        1080,
    )
    .expect("legal frame");
    wc3_game::render::write_ppm(
        out_dir
            .join("headless-NOT-DEVICE-ghost-legal-2414x1080.ppm")
            .to_str()
            .expect("utf8 path"),
        2414,
        1080,
        &legal,
    );
    let stats = region_stats(&legal, &baseline, 2414, region);
    assert!(
        stats.total > 0 && stats.changed * 100 >= stats.total * 3,
        "ghost must visibly repaint its region: changed={}/{} region={region:?}",
        stats.changed,
        stats.total
    );
    assert!(
        stats.changed >= 5_000,
        "ghost repaint must be a real model, not stray pixels: {}",
        stats.changed
    );

    // Blocked preview: the red tint must increase red-hue pixels in its box.
    let mut blocked_report = None;
    if let Some(blocked_at) = blocked {
        game.begin_placement(def_id);
        game.placing_at = Some(blocked_at);
        if !game.ghost().is_some_and(|ghost| !ghost.legal) {
            game.placing = None;
            game.placing_at = None;
        } else if let Some(region_b) = ghost_region(&game, def_id, blocked_at) {
            let scene = game.scene();
            let bad = wc3_game::render::render_scene_to_rgba(
                &game.assets,
                &game.font.atlas,
                game.assets.minimap.as_ref(),
                &scene,
                2414,
                1080,
            )
            .expect("blocked frame");
            wc3_game::render::write_ppm(
                out_dir
                    .join("headless-NOT-DEVICE-ghost-blocked-2414x1080.ppm")
                    .to_str()
                    .expect("utf8 path"),
                2414,
                1080,
                &bad,
            );
            let red_bad = region_stats(&bad, &baseline, 2414, region_b).red;
            let red_legal = region_stats(&legal, &baseline, 2414, region_b).red;
            assert!(
                red_bad > red_legal,
                "blocked ghost must be redder: blocked={red_bad} legal={red_legal} region={region_b:?}"
            );
            blocked_report = Some((red_bad, red_legal));
        }
        game.placing = None;
        game.placing_at = None;
    }

    eprintln!(
        "ART10_GPU_VISIBLE source={source} def={def_id} at=({:.0},{:.0}) region={region:?} \
         changed={}/{} ({:.1}%) red={}{} PASS",
        legal_at.x,
        legal_at.y,
        stats.changed,
        stats.total,
        stats.changed as f64 * 100.0 / stats.total.max(1) as f64,
        stats.red,
        blocked_report
            .map(|(bad, good)| format!(" blocked={bad} legal_same={good}"))
            .unwrap_or_default()
    );
}
