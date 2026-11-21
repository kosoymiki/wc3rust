//! World-frame capture probe (LEDGER 502, world render lane).
//!
//! Renders real battle frames at retail resolution (default 2414x1080) with the
//! shipped assets and writes PNGs plus the per-frame counters. Every render
//! lane before/after pair is produced by this binary, so the camera, map, seed
//! and renderer are identical between the halves of the pair: one `Assets::load`,
//! one headless device, one `Renderer`, many shots.
//!
//! ```text
//! scripts/h2-cargo run --release -p wc3-game --example world_frame -- \
//!     --map analysis/war3_full3/Maps/Campaign/Orc01.w3m \
//!     --shot dawn:6:none --shot noon:12:none --shot dusk:18:none
//! ```
//!
//! Flags:
//! * `--dir DIR`        output directory (default `analysis/agent-re/render`).
//! * `--prefix NAME`    file-name prefix (default `world`).
//! * `--width/--height` frame size (default 2414x1080).
//! * `--shot label:hour:fog` repeatable; fog is `none`, `sim` (the player's real
//!   visibility), `three` (visible|explored|unexplored bands) or a uniform
//!   `visible`/`explored`/`unexplored` mask for ratio measurements.
//! * `--identity`       ungraded reference light.
//! * `--no-overlays`    `H2_TERRAIN_OVERLAYS=off` A/B.
//! * `--no-hud`         blank the UI batch (world-only evidence).
//! * `--map PATH`       map file (default Orc01, then Human04).
//! * `--install PATH`   install directory holding `War3x.mpq`.
//!
//! Per shot the printed line is the machine-readable evidence record:
//! `WORLD_FRAME label=.. hour=.. draws=.. tris=.. band_luma=.. path=..`.

use std::path::PathBuf;

use wc3_game::{Assets, Game};
use wc3_render::fog::FogMask;

struct Shot {
    label: String,
    hour: f32,
    fog: String,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dir = PathBuf::from("analysis/agent-re/render");
    let mut prefix = String::from("world");
    let mut width = 2414u32;
    let mut height = 1080u32;
    let mut identity = false;
    let mut overlays_off = false;
    let mut hud = true;
    let mut shots: Vec<Shot> = Vec::new();
    let mut map_arg: Option<PathBuf> = None;
    let mut install_arg: Option<PathBuf> = None;
    let mut curve = false;
    let mut stats = false;
    let mut target: Option<(f32, f32)> = None;
    let mut distance: Option<f32> = None;
    // `--resolve htow,hpea,oC03`: prove the on-demand model path on a real
    // install before a single frame is drawn (LEDGER 520 acceptance).
    let mut resolve_ids: Vec<String> = Vec::new();
    // `--units`: census of the spawned defs and their world positions, so an
    // evidence camera can be aimed at the scene instead of the map edge.
    let mut dump_units = false;
    // `--warmup N`: frames simulated before the scene snapshot. A campaign map
    // spawns scripted units over time, so a 1 s warmup misses them.
    let mut warmup = 30u32;
    // `--spawn def@x,z`: place a real def in the scene (controlled before/after
    // of the placeholder path on one camera; the unit is sim-authoritative).
    let mut spawns: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = PathBuf::from(args.next().expect("--dir path")),
            "--prefix" => prefix = args.next().expect("--prefix"),
            "--width" => width = args.next().expect("width").parse().expect("width"),
            "--height" => height = args.next().expect("height").parse().expect("height"),
            "--identity" => identity = true,
            "--no-overlays" => overlays_off = true,
            "--no-hud" => hud = false,
            "--curve" => curve = true,
            "--stats" => stats = true,
            "--target" => {
                let spec = args.next().expect("--target X,Z");
                let mut parts = spec.split(',');
                let x: f32 = parts.next().expect("target x").parse().expect("target x");
                let z: f32 = parts.next().expect("target z").parse().expect("target z");
                target = Some((x, z));
            }
            "--distance" => {
                distance = Some(args.next().expect("distance").parse().expect("distance"))
            }
            "--map" => map_arg = Some(PathBuf::from(args.next().expect("--map"))),
            "--install" => install_arg = Some(PathBuf::from(args.next().expect("--install"))),
            "--resolve" => resolve_ids.extend(
                args.next()
                    .expect("--resolve id[,id]")
                    .split(',')
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string),
            ),
            "--units" => dump_units = true,
            "--warmup" => {
                warmup = args.next().expect("--warmup frames").parse().expect("warmup");
            }
            "--spawn" => spawns.push(args.next().expect("--spawn def@x,z")),
            "--shot" => {
                let spec = args.next().expect("--shot label:hour:fog");
                let mut parts = spec.split(':');
                let label = parts.next().expect("shot label").to_string();
                let hour: f32 = parts.next().expect("shot hour").parse().expect("shot hour");
                let fog = parts.next().unwrap_or("none").to_string();
                shots.push(Shot { label, hour, fog });
            }
            other => panic!("unknown flag {other}"),
        }
    }
    if shots.is_empty() {
        shots.push(Shot {
            label: "frame".into(),
            hour: 12.0,
            fog: "none".into(),
        });
    }
    if overlays_off {
        // The terrain mesh is built once in `TerrainRenderer::new`; the switch
        // has to be in place before the renderer exists.
        unsafe { std::env::set_var("H2_TERRAIN_OVERLAYS", "off") };
    }
    if curve {
        // The authored DNC curve, sampled every half hour: dawn/dusk captures
        // must land on the ramp, and 06:00 is already full day in the KLAC key
        // (the previous lane's finding), so the hour is chosen from data.
        let curve = wc3_render::daynight::DayNightCurve::new(wc3_render::daynight::Preset::Lordaeron);
        for step in 0..48 {
            let hour = step as f32 * 0.5;
            let sample = curve.sample(hour);
            let lit = sample.terrain.lit(wc3_render::daynight::FLAT_NDL);
            println!(
                "DNC hour={hour:05.2} sun=({:.3},{:.3},{:.3}) ambient=({:.3},{:.3},{:.3}) \
                 flat_lit=({:.3},{:.3},{:.3}) luma={:.4} fog_density={:.4}",
                sample.terrain.sun[0],
                sample.terrain.sun[1],
                sample.terrain.sun[2],
                sample.terrain.ambient[0],
                sample.terrain.ambient[1],
                sample.terrain.ambient[2],
                lit[0],
                lit[1],
                lit[2],
                lit[0] * 0.2126 + lit[1] * 0.7152 + lit[2] * 0.0722,
                sample.fog_density
            );
        }
    }

    let Some((mpq, map)) = locate(map_arg, install_arg) else {
        eprintln!("SKIP world_frame: retail install or map not present");
        std::process::exit(2);
    };
    let started = std::time::Instant::now();
    let mut assets = Assets::load(Some(&mpq), Some(map.as_path()));
    let load_ms = started.elapsed().as_millis();
    if stats {
        // Map/asset census for the gap report: what the map actually carries.
        let cells_w = assets.map_width_cells.max(1);
        let cells_h = assets.map_height_cells.max(1);
        let water = assets
            .terrain_tiles
            .iter()
            .filter(|tile| tile.is_water())
            .count();
        let tags = assets
            .terrain
            .as_ref()
            .map(|terrain| terrain.ground_tiles.len())
            .unwrap_or(0);
        let mut layers = std::collections::BTreeMap::new();
        for tile in &assets.terrain_tiles {
            *layers.entry(tile.cliff_level()).or_insert(0usize) += 1;
        }
        let doodads = wc3_game::doodads::DoodadWorld::load(&assets);
        // Doodad layers that bind the white stub: a MATERIAL producer of flat
        // pale quads, separate from a unit placeholder. Counted per mesh layer.
        let doodad_white_layers = doodads
            .models
            .iter()
            .flat_map(|model| model.meshes.iter())
            .flat_map(|mesh| mesh.layers.iter())
            .filter(|layer| layer.texture == 0)
            .count();
        println!(
            "WORLD_STATS map={} name={:?} source={} cells={cells_w}x{cells_h} tiles={} \
             water_cells={water} ground_tags={tags} layers={:?} models={} textures={} \
             doodads_parsed={} doodads_placed={} doodad_models={} doodad_textures={} \
             doodad_texture_failures={} doodad_white_layers={doodad_white_layers} \
             expansion={} load_ms={load_ms} layers_mounted={:?}",
            map.display(),
            assets.map_name,
            assets.source,
            assets.terrain_tiles.len(),
            layers,
            assets.models.len(),
            assets.textures.len(),
            doodads.stats.parsed,
            doodads.stats.placed,
            doodads.models.len(),
            doodads.textures.len(),
            doodads.stats.texture_failures,
            assets.is_expansion(),
            assets
                .debug
                .iter()
                .find(|line| line.starts_with("storm_layers="))
                .cloned()
                .unwrap_or_default()
        );
    }
    // On-demand model proof: `ensure_model` is the exact path the per-frame
    // residency scan calls (game.rs) for a def the load did not scope. Print
    // the whole transition — placeholder before, parsed meshes/textured layers
    // after — so the ledger evidence is a line, not a claim.
    for id in &resolve_ids {
        let before = assets.is_placeholder(id);
        let object_model = assets.objects.as_ref().and_then(|objects| {
            objects
                .units()
                .find(|view| view.id_str() == *id)
                .and_then(|view| view.model_file())
        });
        let def = wc3_sim::data::def_of(id).map(|def| (def.model_present, def.model));
        let ensured = assets.ensure_model(id);
        let after = assets.is_placeholder(id);
        let asset = assets.model_for_def(id);
        let meshes = asset.meshes.len();
        let layers: usize = asset.meshes.iter().map(|mesh| mesh.layers.len()).sum();
        let textured = asset
            .meshes
            .iter()
            .flat_map(|mesh| mesh.layers.iter())
            .filter(|layer| layer.texture != 0)
            .count();
        println!(
            "WORLD_RESOLVE def={id} before_placeholder={before} ensure={ensured} \
             after_placeholder={after} meshes={meshes} layers={layers} textured_layers={textured} \
             object_model={object_model:?} def_model={def:?}"
        );
    }

    let mut game = Game::new(assets, 0x5EED);
    for spec in &spawns {
        let Some((def, rest)) = spec.split_once('@') else {
            panic!("--spawn wants def@x,z, got {spec}");
        };
        let mut parts = rest.split(',');
        let x: f32 = parts.next().expect("spawn x").parse().expect("spawn x");
        let z: f32 = parts.next().expect("spawn z").parse().expect("spawn z");
        let ok = game
            .sim
            .spawn_def(0, def, glam::Vec2::new(x, z))
            .is_some();
        println!("WORLD_SPAWN def={def} x={x:.0} z={z:.0} ok={ok}");
    }
    game.enter_battle();
    game.resize(width as f32, height as f32);
    for _ in 0..warmup {
        game.update(1.0 / 30.0);
    }
    if dump_units {
        for unit in game.sim.alive_units() {
            println!(
                "WORLD_UNIT def={} owner={} pos=({:.0},{:.0}) building={} worker={}",
                unit.def().id,
                unit.owner,
                unit.pos.x,
                unit.pos.y,
                unit.is_building(),
                unit.def().stats.is_worker
            );
        }
    }
    let mut scene = game.scene();
    // The acceptance metric for the residency work: a live unit may never draw
    // the bounded placeholder once its first frame is on screen. Both numbers
    // are measured on the scene the renderer is about to consume.
    {
        let assets = &game.assets;
        let placeholder_draws = scene
            .units
            .iter()
            .filter(|draw| {
                assets
                    .models
                    .get(draw.model)
                    .map(|model| model.fallback)
                    .unwrap_or(true)
            })
            .count();
        let mut live_placeholders: Vec<String> = Vec::new();
        for unit in game.sim.alive_units() {
            let def = unit.def().id;
            if assets.is_placeholder(def) && !live_placeholders.iter().any(|id| id == def) {
                live_placeholders.push(def.to_string());
            }
        }
        println!(
            "WORLD_MODELS scope={} loaded={} placeholders_total={} scene_draws={} \
             placeholder_draws={placeholder_draws} live_placeholders={live_placeholders:?} \
             resolve={} skip={}",
            assets.model_scope.summary(),
            assets.loaded_model_count(),
            assets.placeholder_count(),
            scene.units.len(),
            assets.model_resolve_log().len(),
            assets.model_skip_log().len()
        );
        for line in assets.model_skip_log() {
            println!("{line}");
        }
        // White-geoset census: a resident MDX whose geoset has no real texture
        // layer (only the white mask, not a team-colour layer) draws as a flat
        // lit quad. This names the producer instead of guessing from pixels.
        let mut white_geosets: Vec<String> = Vec::new();
        for (def, index) in &assets.model_by_def {
            let model = &assets.models[*index];
            if model.fallback {
                continue;
            }
            let white = model
                .meshes
                .iter()
                .filter(|mesh| {
                    !mesh.layers.is_empty()
                        && mesh
                            .layers
                            .iter()
                            .all(|layer| layer.texture == 0 && !layer.team_color)
                })
                .count();
            if white > 0 {
                white_geosets.push(format!("{def}:{white}"));
            }
        }
        println!(
            "WORLD_WHITE_GEOSETS defs={} list={white_geosets:?}",
            white_geosets.len()
        );
    }
    if let Some((x, z)) = target {
        // Explicit look-at so an evidence frame can be aimed at a map feature
        // (a shoreline, a cliff terrace) instead of the spawn camera.
        let mut camera = wc3_game::Camera::new(game.assets.world_size);
        camera.viewport = glam::Vec2::new(width as f32, height as f32);
        camera.target = glam::Vec2::new(x, z);
        if let Some(value) = distance {
            camera.distance = value;
        }
        scene.view_proj = camera.view_proj();
        scene.eye = camera.eye();
    } else if let Some(value) = distance {
        let mut camera = wc3_game::Camera::new(game.assets.world_size);
        camera.viewport = glam::Vec2::new(width as f32, height as f32);
        camera.distance = value;
        scene.view_proj = camera.view_proj();
        scene.eye = camera.eye();
    }
    if !hud {
        scene.ui.white.clear();
        scene.ui.font.clear();
        scene.ui.minimap.clear();
        scene.ui.textured.clear();
    }
    eprintln!(
        "WORLD_SETUP load_ms={load_ms} map={} size={width}x{height} units={} rings={}",
        map.display(),
        scene.units.len(),
        scene.rings.len()
    );
    std::fs::create_dir_all(&dir).expect("create output dir");

    // One device and one renderer for every shot: the 730 texture uploads and
    // the terrain mesh build dominate a cold frame, and re-creating them per
    // shot would also make the frames non-comparable (different mip/state).
    let headless = wc3_game::render::HeadlessTarget::new(width, height)
        .unwrap_or_else(|error| panic!("headless device: {error}"));
    let mut renderer = wc3_game::render::Renderer::new(
        &headless.device,
        &headless.queue,
        headless.format,
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
    )
    .unwrap_or_else(|error| panic!("renderer: {error}"));
    renderer.set_identity_light(identity);
    renderer.resize(&headless.device, headless.width, headless.height);

    for shot in &shots {
        renderer.set_time_of_day(shot.hour);
        match shot.fog.as_str() {
            "none" => renderer.clear_fog(),
            "sim" => renderer.set_fog(&game.sim, 0),
            "three" => renderer.set_fog_mask(banded_mask(&game)),
            "visible" => renderer.set_fog_mask(uniform_mask(&game, 2)),
            "explored" => renderer.set_fog_mask(uniform_mask(&game, 1)),
            "unexplored" => renderer.set_fog_mask(uniform_mask(&game, 0)),
            other => panic!("unknown fog mode {other}"),
        }
        let began = std::time::Instant::now();
        headless.render(&mut renderer, &scene, wc3_game::render::RenderOptions::default());
        let rgba = match headless.read() {
            Ok(rgba) => rgba,
            Err(error) => {
                eprintln!("FAIL world_frame {}: {error}", shot.label);
                std::process::exit(1);
            }
        };
        let path = dir.join(format!(
            "{prefix}-{}-{width}x{height}.png",
            shot.label
        ));
        write_png(&path, &rgba, width, height);
        let (band, full) = means(&rgba, width, height);
        let white = near_white_band(&rgba, width, height);
        let tan = tan_band(&rgba, width, height);
        let band_len = band_len(width, height).max(1) as f64;
        let counters = renderer.counters();
        println!(
            "WORLD_FRAME label={} size={width}x{height} hour={:.2} identity={identity} fog={} \
             draws={} passes={} texture_binds={} vertices={} tris={} upload_bytes={} \
             band_rgb={:.2},{:.2},{:.2} band_luma={:.2} band_near_white={} band_near_white_pct={:.2} \
             band_tan={} band_tan_pct={:.2} \
             full_rgb={:.2},{:.2},{:.2} full_luma={:.2} frame_ms={} path={}",
            shot.label,
            shot.hour,
            shot.fog,
            counters.draws,
            counters.passes,
            counters.texture_binds,
            counters.vertices,
            counters.indices / 3,
            counters.upload_bytes,
            band[0],
            band[1],
            band[2],
            luma(band),
            white,
            100.0 * white as f64 / band_len,
            tan,
            100.0 * tan as f64 / band_len,
            full[0],
            full[1],
            full[2],
            luma(full),
            began.elapsed().as_millis(),
            path.display()
        );
    }
}

fn write_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).unwrap_or_else(|error| {
        panic!("create {}: {error}", path.display());
    });
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
}

fn locate(map_arg: Option<PathBuf>, install_arg: Option<PathBuf>) -> Option<(PathBuf, PathBuf)> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // `analysis/agent-assets/wc3-install/War3x.mpq` is a 34-byte placeholder in
    // this checkout; a real install is ~300 MB, so the size floors the candidate
    // list and a stub can never be "loaded" as an archive.
    fn real(path: &std::path::Path) -> bool {
        path.metadata().map(|meta| meta.len() > 1_000_000).unwrap_or(false)
    }
    let mpq = install_arg
        .map(|dir| dir.join("War3x.mpq"))
        .filter(|path| real(path))
        .or_else(|| {
            std::env::var_os("H2_WC3_INSTALL")
                .map(PathBuf::from)
                .map(|dir| dir.join("War3x.mpq"))
                .filter(|path| real(path))
        })
        .or_else(|| {
            [
                // The install *directory* is the canonical source: `Assets::load`
                // mounts every archive next to the file it is given, so the
                // `wc3-install` symlinks pull in War3.mpq + War3x.mpq +
                // War3xlocal.mpq. A standalone `War3x.mpq` copy has no base
                // archive beside it, and RoC maps then lose their base textures
                // (doodad layers fell back to the white placeholder).
                repo.join("analysis/agent-assets/wc3-install/War3x.mpq"),
                PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3x.mpq"),
                repo.join("analysis/tft_files_Files100_War3x.mpq"),
            ]
            .into_iter()
            .find(|path| real(path))
        })?;
    let map = map_arg
        .filter(|path| path.is_file())
        .or_else(|| std::env::var_os("H2_WC3_MAP").map(PathBuf::from).filter(|p| p.is_file()))
        .or_else(|| {
            [
                repo.join("analysis/war3_full3/Maps/Campaign/Orc01.w3m"),
                repo.join("analysis/war3_map/Maps/Campaign/Human04.w3m"),
                repo.join("donors/maps/melee/(2)Justice.w3x"),
            ]
            .into_iter()
            .find(|path| path.is_file())
        })?;
    Some((mpq, map))
}

fn banded_mask(game: &Game) -> FogMask {
    let (gw, gh, cell) = grid(game);
    let mut cells = Vec::with_capacity((gw * gh) as usize);
    for _z in 0..gh {
        for x in 0..gw {
            // Thirds across the map: visible | explored | unexplored.
            let state = if x * 3 < gw {
                2
            } else if x * 3 < gw * 2 {
                1
            } else {
                0
            };
            cells.push(state);
        }
    }
    FogMask::new(gw, gh, cell, cells)
}

fn uniform_mask(game: &Game, state: u8) -> FogMask {
    let (gw, gh, cell) = grid(game);
    FogMask::new(gw, gh, cell, vec![state; (gw * gh) as usize])
}

fn grid(game: &Game) -> (u32, u32, f32) {
    let stride = (game.sim.grid.width.max(game.sim.grid.height) / 256).max(1);
    (
        game.sim.grid.width.div_ceil(stride).max(1),
        game.sim.grid.height.div_ceil(stride).max(1),
        game.sim.grid.cell_size * stride as f32,
    )
}

/// Pixels of the terrain band (42..82 % of the frame height).
fn band_len(width: u32, height: u32) -> u64 {
    let top = (height as f64 * 0.42) as u32;
    let bottom = (height as f64 * 0.82) as u32;
    u64::from(bottom.saturating_sub(top)) * u64::from(width)
}

/// Near-white pixels in the terrain band: every channel >= 235, the metric the
/// white-doodad / placeholder-cube defects are measured with (the ledger-502
/// baseline counted RGB-247 patches this way). The HUD is outside the band, so
/// its white text does not pollute the number.
fn near_white_band(rgba: &[u8], width: u32, height: u32) -> u64 {
    let top = (height as f64 * 0.42) as u32;
    let bottom = (height as f64 * 0.82) as u32;
    let mut count = 0u64;
    for y in top..bottom {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            if rgba[i] >= 235 && rgba[i + 1] >= 235 && rgba[i + 2] >= 235 {
                count += 1;
            }
        }
    }
    count
}

/// Bright **warm-neutral** pixels in the terrain band: the white mask lit by
/// the warm sun reads as pale tan (~231,197,167) rather than pure white, so the
/// placeholder-cube metric needs this second bucket. Terrain sand and sky stay
/// below the 205 red floor; the pale quad patches do not.
fn tan_band(rgba: &[u8], width: u32, height: u32) -> u64 {
    let top = (height as f64 * 0.42) as u32;
    let bottom = (height as f64 * 0.82) as u32;
    let mut count = 0u64;
    for y in top..bottom {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let (r, g, b) = (rgba[i], rgba[i + 1], rgba[i + 2]);
            if r >= 205 && g >= 170 && b >= 140 && r >= g && g >= b && r - b <= 90 {
                count += 1;
            }
        }
    }
    count
}

/// Mean colour of the terrain band (42..82 % of the frame height) and the full
/// frame. The band is the world: above it is sky, below it is the HUD.
fn means(rgba: &[u8], width: u32, height: u32) -> ([f64; 3], [f64; 3]) {
    let top = (height as f64 * 0.42) as u32;
    let bottom = (height as f64 * 0.82) as u32;
    let mut band = [0.0f64; 3];
    let mut band_pixels = 0u64;
    let mut full = [0.0f64; 3];
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            for channel in 0..3 {
                let value = rgba[i + channel] as f64;
                full[channel] += value;
                if y >= top && y < bottom {
                    band[channel] += value;
                }
            }
            if y >= top && y < bottom {
                band_pixels += 1;
            }
        }
    }
    let pixels = (width as u64 * height as u64).max(1) as f64;
    let band_pixels = band_pixels.max(1) as f64;
    for channel in 0..3 {
        band[channel] /= band_pixels;
        full[channel] /= pixels;
    }
    (band, full)
}

fn luma(mean: [f64; 3]) -> f64 {
    mean[0] * 0.2126 + mean[1] * 0.7152 + mean[2] * 0.0722
}
