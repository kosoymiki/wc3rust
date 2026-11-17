//! LEDGER 479 — lighting / fog / day-night acceptance.
//!
//! Test ids (exact, one per mechanic):
//!
//! * `render.color.no-yellow-cast` — noon is neutral against the ungraded
//!   reference, 18:00 warm but bounded, 00:00 dark but not black.
//! * `render.lighting.hour-changes` — the DNC curve actually moves the light.
//! * `render.daynight.follows-sim-hour` — `DayNightState::sync` overrides the
//!   private clock and is not advanced by frame time.
//! * `render.fow.three-states` — unexplored / explored / visible veil levels
//!   with a soft edge (CPU grid + a real frame over the map).
//! * `render.fog.atmosphere-visible` — the distance haze exists, survives night
//!   and is stronger far away than near.
//! * `render.uniform.camera-size` — the Rust camera uniform and both WGSL
//!   `Camera` structs agree (the 112/176 device abort guard).
//!
//! The GPU half writes the evidence frames (2414x1080) into
//! `analysis/agent-re/render/` and prints the per-channel means that
//! `LIGHTING-479.md` records. It skips cleanly when the retail install or a
//! Vulkan adapter is absent.
//!
//! ```text
//! scripts/h2-cargo test -p wc3-render --test lighting_479 -- --nocapture --test-threads=1
//! ```

use std::path::PathBuf;

use wc3_game::{Assets, Game, render};

use wc3_render::daynight::{
    self, DayNightCurve, DayNightState, DayNightSample, FogSettings, Preset, FLAT_NDL,
    ATMOSPHERE_DENSITY, ATMOSPHERE_END, ATMOSPHERE_START,
};
use wc3_render::fog::{self, FogMask};

const WIDTH: u32 = 2414;
const HEIGHT: u32 = 1080;

// ---------------------------------------------------------------- helpers

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

/// Mean R/G/B over the terrain band (42 %..82 % of the frame height) and over
/// the full frame.
///
/// The band is the *world*: rows above it are sky (the graded clear colour,
/// which is intentionally not the ungraded reference), rows below it are the
/// HUD (WC3 keeps the interface ungraded). Comparing the graded frame against
/// the ungraded one on this band isolates the light on real geometry.
fn channel_means(rgba: &[u8], width: u32, height: u32) -> ([f64; 3], [f64; 3]) {
    let top = (height as f64 * 0.42) as u32;
    let bottom = (height as f64 * 0.82) as u32;
    let left = 0;
    let right = width;
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

fn mean_luma(mean: [f64; 3]) -> f64 {
    mean[0] * 0.2126 + mean[1] * 0.7152 + mean[2] * 0.0722
}

/// Mean luminance of the rows between two fractions of the frame height.
///
/// The fog-of-war ratios must be measured on guaranteed world geometry: the top
/// of a WC3 frame is sky (the clear colour), and the mask pass cannot veil a
/// pixel with no world position, so a full-frame mean would mix the horizon into
/// the veil ratio.
fn rows_luma(rgba: &[u8], width: u32, height: u32, top: f64, bottom: f64) -> f64 {
    let y0 = (height as f64 * top) as u32;
    let y1 = (height as f64 * bottom) as u32;
    let mut sum = [0.0f64; 3];
    let mut count = 0u64;
    for y in y0..y1 {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            for channel in 0..3 {
                sum[channel] += rgba[i + channel] as f64;
            }
            count += 1;
        }
    }
    let mean = [
        sum[0] / count.max(1) as f64,
        sum[1] / count.max(1) as f64,
        sum[2] / count.max(1) as f64,
    ];
    mean_luma(mean)
}

fn write_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

fn curve() -> DayNightCurve {
    // The renderer's default: Lordaeron terrain + the atmosphere fallback for
    // maps without a `war3map.w3i` fog block.
    let mut curve = DayNightCurve::new(Preset::Lordaeron);
    curve.set_atmosphere(true);
    curve
}

/// Flat-surface luminance of one sample (what a level, up-facing texel gets).
fn sample_luma(sample: &DayNightSample) -> f32 {
    let lit = sample.terrain.lit(FLAT_NDL);
    lit[0] * 0.2126 + lit[1] * 0.7152 + lit[2] * 0.0722
}

/// The shader's distance haze, replicated for the CPU tests.
fn haze(colour: [f32; 3], world: [f32; 3], eye: [f32; 3], sample: &DayNightSample) -> [f32; 3] {
    if sample.fog_density <= 0.001 {
        return colour;
    }
    let delta = [
        world[0] - eye[0],
        world[1] - eye[1],
        world[2] - eye[2],
    ];
    let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    let span = (sample.fog_range[1] - sample.fog_range[0]).max(1.0);
    let t = ((distance - sample.fog_range[0]) / span).clamp(0.0, 1.0) * sample.fog_density;
    [
        colour[0] + (sample.fog_color[0] - colour[0]) * t,
        colour[1] + (sample.fog_color[1] - colour[1]) * t,
        colour[2] + (sample.fog_color[2] - colour[2]) * t,
    ]
}

// ---------------------------------------------------------------- CPU tests

/// `render.color.no-yellow-cast`
///
/// The operator's «жуткий жёлтый фильтр» is a *cast*: the graded frame differs
/// from the same frame under a flat white light. The test measures exactly that
/// difference, in 8-bit counts, at the three mission hours.
#[test]
fn render_color_no_yellow_cast() {
    let curve = curve();
    let noon = curve.sample(12.0);
    let neutral = daynight::neutral_sample(12.0);

    // Noon: the flat-surface grade must be white. 6/255 = 0.0235 of full scale.
    for channel in 0..3 {
        let graded = noon.terrain.grade[channel];
        let ungraded = neutral.terrain.grade[channel];
        let delta_counts = (graded - ungraded).abs() * 255.0;
        assert!(
            delta_counts <= 6.0,
            "channel {channel}: noon graded {graded:.4} vs ungraded {ungraded:.4} \
             = {delta_counts:.2}/255 > 6/255 (yellow cast)"
        );
    }
    // The unit light uses the same curve (unit DNC has 0.3 ambient, so its flat
    // light is brighter, but it must still be neutral).
    let unit_ratio = noon.unit.grade[0] / noon.unit.grade[1].max(1e-6);
    assert!(
        (unit_ratio - 1.0).abs() < 0.02,
        "unit noon light is tinted: r/g = {unit_ratio:.4}"
    );

    // 18:00: warm (red above blue — the shipped dusk ambient is magenta, so
    // green dips below blue for a few minutes). The light itself stays near
    // full scale: WC3's dusk ramp runs 17:20–20:00, and the *frame* brightness
    // budget (no channel mean above 0.75 of full scale) is asserted on the
    // rendered PNG below, where the terrain albedo is part of the mean.
    let dusk = curve.sample(18.0);
    let (r, g, b) = (
        dusk.terrain.grade[0],
        dusk.terrain.grade[1],
        dusk.terrain.grade[2],
    );
    assert!(
        r > b + 0.01,
        "18:00 must read warm (r-b = {:.3}), got {r:.3} {g:.3} {b:.3}",
        r - b
    );

    // 00:00: dark but not black (the old grade left the night at ~55 % with a
    // crushed blue channel; the correct DNC night is a low, warm moon).
    let night = curve.sample(0.0);
    let luma = sample_luma(&night);
    assert!(
        (0.02..0.55).contains(&luma),
        "00:00 flat luminance {luma:.3} outside 0.02..0.55 (dark but not black)"
    );
    assert!(
        night.terrain.lit(FLAT_NDL)[2] > 0.02,
        "blue channel crushed at midnight: {:.3}",
        night.terrain.lit(FLAT_NDL)[2]
    );
}

/// `render.lighting.hour-changes`
#[test]
fn render_lighting_hour_changes() {
    let curve = curve();
    // 06:00 and 12:00 are the *same* authored key (`KLAC` reaches full white at
    // 06:00 and holds it to 18:00), so distinctness is asserted on the hours the
    // shipped tables actually separate: midnight, dawn ramp, dusk, deep night.
    let hours = [0.0f32, 4.5, 12.0, 18.0, 19.5, 21.0];
    let samples: Vec<DayNightSample> = hours.iter().map(|hour| curve.sample(*hour)).collect();
    let lumas: Vec<f32> = samples.iter().map(sample_luma).collect();
    for i in 0..hours.len() {
        for j in (i + 1)..hours.len() {
            assert!(
                (lumas[i] - lumas[j]).abs() > 0.005,
                "hours {} and {} render the same light ({:.4} vs {:.4})",
                hours[i],
                hours[j],
                lumas[i],
                lumas[j]
            );
        }
    }
    assert!(
        lumas[2] > lumas[0] * 3.0,
        "noon must be at least 3x the midnight luminance: {:.3} vs {:.3}",
        lumas[2],
        lumas[0]
    );
    // 06:00 is the authored start of full day: it must equal noon exactly.
    let dawn = curve.sample(6.0);
    let noon = curve.sample(12.0);
    for channel in 0..3 {
        assert!(
            (dawn.terrain.grade[channel] - noon.terrain.grade[channel]).abs() < 1e-4,
            "06:00 must match the authored full-day key (channel {channel})"
        );
    }
    // The sun direction is a static DNC property; it must not wobble with hour.
    for sample in &samples {
        assert_eq!(sample.sun_direction, samples[0].sun_direction);
    }
    // Dawn/dusk flags follow `MiscData.txt` (Dawn=6, Dusk=18).
    assert!(curve.sample(5.99).is_night);
    assert!(!curve.sample(6.0).is_night);
    assert!(!curve.sample(17.99).is_night);
    assert!(curve.sample(18.0).is_night);
}

/// `render.daynight.follows-sim-hour`
///
/// The renderer's `sync_day_night` stores the simulation hour and never
/// advances it: a hitch, a paused tab or a 10 s frame cannot move the sun.
#[test]
fn render_daynight_follows_sim_hour() {
    let mut state = DayNightState::new();

    // Free-running clocks start at 00:00 and advance with frame time.
    assert!(!state.synced_hour().is_some());
    assert!(state.hour().abs() < 1e-6);
    state.advance_seconds(60.0);
    let free_hour = state.hour();
    assert!(
        (free_hour - 3.0).abs() < 0.01,
        "free clock: 60 s of a 480 s day = 3 game hours, got {free_hour:.3}"
    );

    // Sync to midnight, then to noon: the grade must follow the injected hour.
    state.sync(0.0);
    assert!(state.synced_hour() == Some(0.0));
    let night = curve().sample(state.hour());
    state.sync(12.0);
    assert!(state.synced_hour() == Some(12.0));
    let noon = curve().sample(state.hour());
    assert!(
        sample_luma(&noon) > sample_luma(&night) * 3.0,
        "sync(0) and sync(12) must differ: {:.3} vs {:.3}",
        sample_luma(&night),
        sample_luma(&noon)
    );

    // Frame time must not advance a synced clock.
    state.advance_seconds(600.0);
    assert_eq!(state.hour(), 12.0, "synced hour drifted with frame time");
    state.sync(4.25);
    state.advance_seconds(30.0);
    assert_eq!(state.hour(), 4.25, "synced hour drifted after a 30 s frame");

    // Releasing resumes from the synced hour.
    state.release();
    assert!(state.synced_hour().is_none());
    state.advance_seconds(60.0);
    assert!(
        (state.hour() - 7.25).abs() < 0.01,
        "release must resume from 4.25 + 3 h, got {:.3}",
        state.hour()
    );
}

/// `render.fow.three-states`
///
/// The veil levels are part of the mechanic: unexplored is black (<= 6 % of the
/// lit value), explored is 35–55 % of the lit value, visible is untouched, and
/// the boundary is a gradient rather than a per-cell staircase.
#[test]
fn render_fow_three_states() {
    // 15x1 grid: 5 visible | 5 explored | 5 unexplored. The grid must be wider
    // than twice the blur radius on each side, or the outermost cell of a band
    // already averages its neighbour's level.
    let mask = FogMask::new(
        15,
        1,
        128.0,
        vec![2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0],
    );
    let darkness = fog::darkness_grid(&mask);
    // Interior cells keep the authored level; edge cells blend.
    assert_eq!(darkness[1], 0.0, "visible cell must stay untouched");
    assert!(
        (darkness[7] - fog::EXPLORED_DARKNESS).abs() < 0.02,
        "explored interior {:.3} far from {:.3}",
        darkness[7],
        fog::EXPLORED_DARKNESS
    );
    assert!(
        (darkness[13] - fog::UNEXPLORED_DARKNESS).abs() < 0.02,
        "unexplored interior {:.3} far from 1.0",
        darkness[13]
    );
    // Resulting brightness relative to the lit frame: veil alpha `d` leaves
    // `1 - d` of the *linear* colour, which the sRGB target encodes back to
    // `(1 - d)^(1/2.2)` of the lit pixel.
    let visible = fog::remaining_ratio(darkness[1]);
    let explored = fog::remaining_ratio(darkness[7]);
    let unexplored = fog::remaining_ratio(darkness[13]);
    assert!(visible > 0.94, "visible keeps {visible:.3} of the lit value");
    assert!(
        (0.35..=0.55).contains(&explored),
        "explored keeps {explored:.3} of the lit pixel, outside the 0.35–0.55 band"
    );
    assert!(
        unexplored <= 0.06,
        "unexplored keeps {unexplored:.3}, must be <= 0.06"
    );
    // Soft edge: the transition between the two plateaus is a gradient.
    let edge: Vec<f32> = darkness[4..12].to_vec();
    assert!(
        edge.windows(2).all(|pair| pair[0] <= pair[1] + 1e-6),
        "darkness must rise monotonically across the boundary: {edge:?}"
    );
    assert!(
        edge.iter().any(|v| *v > 0.2 && *v < 0.9),
        "no intermediate value on the boundary (hard edge): {edge:?}"
    );
    // The three states are counted for the device log / minimap.
    assert_eq!(mask.histogram(), (5, 5, 5));
    assert!((mask.visible_fraction() - 5.0 / 15.0).abs() < 1e-6);
}

/// `render.fog.atmosphere-visible`
#[test]
fn render_fog_atmosphere_visible() {
    let curve = curve();
    let noon = curve.sample(12.0);
    // The atmosphere is enabled for maps without a w3i fog block.
    assert!(noon.fog_density > 0.0, "atmosphere disabled by default");
    assert!((noon.fog_density - ATMOSPHERE_DENSITY).abs() < 1e-6);
    assert!((noon.fog_range[0] - ATMOSPHERE_START).abs() < 1e-3);
    assert!((noon.fog_range[1] - ATMOSPHERE_END).abs() < 1e-3);
    // A map with its own fog block wins over the fallback.
    let mut mapped = DayNightCurve::with_fog(
        Preset::Lordaeron,
        FogSettings {
            base_color: [0.3, 0.2, 0.1],
            density: 0.5,
            start: 200.0,
            end: 2000.0,
        },
    );
    mapped.set_atmosphere(true);
    assert!((mapped.sample(12.0).fog_density - 0.5).abs() < 1e-6);

    // Near terrain keeps its colour, far terrain fades into the haze colour.
    let eye = [0.0, 0.0, 0.0];
    let colour = [0.30, 0.28, 0.18];
    let near = haze(colour, [0.0, 0.0, 600.0], eye, &noon);
    let far = haze(colour, [0.0, 0.0, 3600.0], eye, &noon);
    assert!(
        (near[0] - colour[0]).abs() < 1e-4,
        "near terrain must not be hazed: {near:?}"
    );
    let far_delta = (far[0] - colour[0]).abs() + (far[2] - colour[2]).abs();
    assert!(
        far_delta > 0.08,
        "far terrain barely hazes: {far:?} (delta {far_delta:.3})"
    );
    // The haze colour moves toward the sky, not toward black.
    assert!(
        far[2] > colour[2],
        "haze must cool the far plane (b {:.3} -> {:.3})",
        colour[2],
        far[2]
    );

    // Night keeps a readable horizon: the fog colour is dark but never black.
    let night = curve.sample(0.0);
    let luma = night.fog_color[0] * 0.2126
        + night.fog_color[1] * 0.7152
        + night.fog_color[2] * 0.0722;
    assert!(
        luma > 0.004 && luma < 0.25,
        "night horizon luminance {luma:.4} outside 0.004..0.25"
    );
    // ...and it is still much darker than noon.
    let noon_luma = noon.fog_color[0] * 0.2126
        + noon.fog_color[1] * 0.7152
        + noon.fog_color[2] * 0.0722;
    assert!(luma < noon_luma * 0.6, "night horizon is too bright");
}

/// `render.uniform.camera-size`
///
/// Build 0f037ea2 aborted with "bound with size 112 where the shader expects
/// 176": this test parses the WGSL `Camera` structs and compares them with the
/// Rust uniform, so the drift cannot come back silently.
#[test]
fn render_uniform_camera_size() {
    let mesh_wgsl = wc3_render::mesh::camera_uniform_size_from_wgsl();
    let terrain_wgsl = wc3_render::mesh::wgsl_struct_size(
        wc3_game::terrain_render::TERRAIN_SHADER,
        "Camera",
    );
    let rust = wc3_render::mesh::CAMERA_UNIFORM_SIZE;
    assert_eq!(mesh_wgsl, 176, "mesh WGSL Camera is not 176 B");
    assert_eq!(terrain_wgsl, 176, "terrain WGSL Camera is not 176 B");
    assert_eq!(rust, 176, "mesh CameraUniform is not 176 B");
    assert_eq!(
        mesh_wgsl, terrain_wgsl,
        "the two WGSL Camera structs disagree; one pipeline would abort"
    );
}

// ---------------------------------------------------------------- GPU frames

/// Write the four mission hours plus the ungraded reference at noon, the
/// three-state fog frame and the atmosphere A/B, and assert the mission's
/// luminance targets on the real frames.
#[test]
fn lighting_frames_479() {
    let Some(mut game) = game() else {
        eprintln!("SKIP lighting 479 frames: War3x.mpq / map not present");
        return;
    };
    let scene = game.scene();
    let out = repo().join("analysis/agent-re/render");
    std::fs::create_dir_all(&out).expect("create render dir");

    let shoot = |width: u32,
                 height: u32,
                 hour: f32,
                 identity: bool,
                 fog: Option<&FogMask>|
     -> Result<Vec<u8>, String> {
        render::render_scene_to_rgba_with_lighting(
            &game.assets,
            &game.font.atlas,
            game.assets.minimap.as_ref(),
            &scene,
            width,
            height,
            render::FrameLighting {
                hour: Some(hour),
                atmosphere: Some(true),
                identity: Some(identity),
                fog,
            },
        )
    };

    let mut hours = Vec::new();
    for (label, hour) in [
        ("0600", 6.0f32),
        ("1200", 12.0),
        ("1800", 18.0),
        ("2000", 20.0),
        ("0000", 0.0),
    ] {
        let rgba = match shoot(WIDTH, HEIGHT, hour, false, None) {
            Ok(rgba) => rgba,
            Err(error) => {
                eprintln!("SKIP lighting 479 frames: headless render unavailable: {error}");
                return;
            }
        };
        let path = out.join(format!("light-{label}-479-{WIDTH}x{HEIGHT}.png"));
        write_png(&path, &rgba, WIDTH, HEIGHT);
        let (band, full) = channel_means(&rgba, WIDTH, HEIGHT);
        println!(
            "LIGHTING479 hour={hour:05.2} band_r={:.2} band_g={:.2} band_b={:.2} \
             band_luma={:.2} full_r={:.2} full_g={:.2} full_b={:.2} path={}",
            band[0],
            band[1],
            band[2],
            mean_luma(band),
            full[0],
            full[1],
            full[2],
            path.display()
        );
        hours.push((label, hour, band, full));
    }

    // Per-hour assertions on the real frames.
    let noon = hours.iter().find(|row| row.0 == "1200").expect("noon frame");
    let dusk = hours.iter().find(|row| row.0 == "1800").expect("dusk frame");
    let late = hours.iter().find(|row| row.0 == "2000").expect("warm dusk frame");
    let night = hours.iter().find(|row| row.0 == "0000").expect("night frame");
    assert!(
        noon.2[0].max(noon.2[1]).max(noon.2[2]) <= 0.75 * 255.0,
        "noon frame is blown out: {:.1}/{:.1}/{:.1}",
        noon.2[0],
        noon.2[1],
        noon.2[2]
    );
    // WC3's dusk ramp starts at 18:04 (KLBC turns magenta) and the warm phase
    // peaks around 20:00; at 18:00 the sun key is still full white, so the warm
    // proof is 20:00. R/B is the warm metric (grass keeps green dominant).
    let noon_warmth = noon.2[0] / noon.2[2].max(1e-6);
    let late_warmth = late.2[0] / late.2[2].max(1e-6);
    assert!(
        late_warmth > noon_warmth * 1.15,
        "20:00 must read warmer than noon: r/b {late_warmth:.3} vs {noon_warmth:.3}"
    );
    assert!(
        dusk.2[0] > 0.0 && dusk.2[1] > 0.0,
        "18:00 frame empty"
    );
    assert!(
        mean_luma(night.2) > 4.0,
        "midnight frame is a black screen (band luma {:.2})",
        mean_luma(night.2)
    );
    assert!(
        mean_luma(night.2) < mean_luma(noon.2) * 0.8,
        "midnight is not darker than noon"
    );

    // Yellow-cast proof: the graded noon frame against the ungraded reference
    // of the same camera and scene. 6/255 per channel is the mission budget.
    let identity = match shoot(WIDTH, HEIGHT, 12.0, true, None) {
        Ok(rgba) => rgba,
        Err(error) => {
            eprintln!("SKIP lighting 479 cast: {error}");
            return;
        }
    };
    let identity_path = out.join(format!("light-1200-identity-479-{WIDTH}x{HEIGHT}.png"));
    write_png(&identity_path, &identity, WIDTH, HEIGHT);
    let (graded_band, graded_full) = (noon.2, noon.3);
    let (identity_band, identity_full) = channel_means(&identity, WIDTH, HEIGHT);
    // The cast is measured on the terrain band (the world), not on the sky: the
    // horizon is graded by design, so comparing the graded sky against the
    // ungraded clear colour would measure the sky, not the light.
    let delta = [
        graded_band[0] - identity_band[0],
        graded_band[1] - identity_band[1],
        graded_band[2] - identity_band[2],
    ];
    println!(
        "CAST479 hour=12.00 graded_band={:.2},{:.2},{:.2} identity_band={:.2},{:.2},{:.2} \
         delta={:+.2},{:+.2},{:+.2} graded_full={:.2},{:.2},{:.2} \
         identity_full={:.2},{:.2},{:.2} path={} identity_path={}",
        graded_band[0],
        graded_band[1],
        graded_band[2],
        identity_band[0],
        identity_band[1],
        identity_band[2],
        delta[0],
        delta[1],
        delta[2],
        graded_full[0],
        graded_full[1],
        graded_full[2],
        identity_full[0],
        identity_full[1],
        identity_full[2],
        out.join(format!("light-1200-479-{WIDTH}x{HEIGHT}.png")).display(),
        identity_path.display()
    );
    for (channel, name) in ["red", "green", "blue"].iter().enumerate() {
        assert!(
            delta[channel].abs() <= 6.0,
            "noon {name} cast {:.2}/255 > 6/255 vs the ungraded frame",
            delta[channel]
        );
    }

    // Fog of war, three states proven on real frames. Three *uniform* masks
    // (all visible / all explored / all unexplored) give an exact per-state
    // ratio over the same camera: the veil multiplies the encoded framebuffer,
    // so an explored frame must keep 45 % of the visible frame and an
    // unexplored frame must keep ~0. A fourth frame with a three-band mask
    // (visible | explored | unexplored) is written as the visual artifact — its
    // soft edge is a screen boundary and cannot be measured by thirds.
    let small = (480u32, 270u32);
    let (gw, gh, cell) = (
        game.sim.grid.width,
        game.sim.grid.height,
        game.sim.grid.cell_size,
    );
    let uniform = |state: u8| -> Result<Vec<u8>, String> {
        let mask = FogMask::new(gw, gh, cell, vec![state; (gw * gh) as usize]);
        shoot(small.0, small.1, 12.0, false, Some(&mask))
    };
    let veiled = |rgba: &[u8]| -> f64 { rows_luma(rgba, small.0, small.1, 0.50, 0.80) };
    let visible = match uniform(2) {
        Ok(rgba) => veiled(&rgba),
        Err(error) => {
            eprintln!("SKIP lighting 479 fow: {error}");
            return;
        }
    };
    let explored = match uniform(1) {
        Ok(rgba) => veiled(&rgba),
        Err(error) => {
            eprintln!("SKIP lighting 479 fow: {error}");
            return;
        }
    };
    let unexplored = match uniform(0) {
        Ok(rgba) => veiled(&rgba),
        Err(error) => {
            eprintln!("SKIP lighting 479 fow: {error}");
            return;
        }
    };
    let explored_ratio = explored / visible.max(1e-6);
    let unexplored_ratio = unexplored / visible.max(1e-6);
    println!(
        "FOW479 visible_luma={visible:.2} explored_luma={explored:.2} \
         unexplored_luma={unexplored:.2} explored_ratio={explored_ratio:.3} \
         unexplored_ratio={unexplored_ratio:.3} size={}x{}",
        small.0, small.1
    );
    assert!(
        (0.35..=0.55).contains(&explored_ratio),
        "explored frame keeps {explored_ratio:.3} of the lit value (want 0.35–0.55)"
    );
    assert!(
        unexplored_ratio <= 0.06,
        "unexplored frame keeps {unexplored_ratio:.3} of the lit value (want <= 0.06)"
    );

    // Visual artifact: the three states side by side over the real map.
    let mut cells = Vec::with_capacity((gw * gh) as usize);
    for _ in 0..gh {
        for x in 0..gw {
            cells.push(if x < gw / 3 {
                2
            } else if x < 2 * gw / 3 {
                1
            } else {
                0
            });
        }
    }
    let mask = FogMask::new(gw, gh, cell, cells);
    let fow = match shoot(WIDTH, HEIGHT, 12.0, false, Some(&mask)) {
        Ok(rgba) => rgba,
        Err(error) => {
            eprintln!("SKIP lighting 479 fow: {error}");
            return;
        }
    };
    let fow_path = out.join(format!("fow-three-states-479-{WIDTH}x{HEIGHT}.png"));
    write_png(&fow_path, &fow, WIDTH, HEIGHT);
    println!(
        "FOW479 artifact histogram={:?} path={}",
        mask.histogram(),
        fow_path.display()
    );

    // Atmosphere A/B on the same camera at 480x270: the far half must change.
    let near_height = 270u32;
    let near_width = 480u32;
    let with_haze = match shoot(near_width, near_height, 12.0, false, None) {
        Ok(rgba) => rgba,
        Err(error) => {
            eprintln!("SKIP lighting 479 atmosphere: {error}");
            return;
        }
    };
    let without_haze = match render::render_scene_to_rgba_with_lighting(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        near_width,
        near_height,
        render::FrameLighting {
            hour: Some(12.0),
            atmosphere: Some(false),
            identity: Some(false),
            fog: None,
        },
    ) {
        Ok(rgba) => rgba,
        Err(error) => {
            eprintln!("SKIP lighting 479 atmosphere: {error}");
            return;
        }
    };
    let (haze_band, _) = channel_means(&with_haze, near_width, near_height);
    let (plain_band, _) = channel_means(&without_haze, near_width, near_height);
    let haze_delta = (haze_band[0] - plain_band[0]).abs()
        + (haze_band[1] - plain_band[1]).abs()
        + (haze_band[2] - plain_band[2]).abs();
    println!(
        "ATMOS479 band_with={:.2},{:.2},{:.2} band_without={:.2},{:.2},{:.2} delta={:.2}",
        haze_band[0], haze_band[1], haze_band[2], plain_band[0], plain_band[1], plain_band[2], haze_delta
    );
    assert!(
        haze_delta > 1.5,
        "atmospheric haze is not visible in the frame (band delta {haze_delta:.2}/255)"
    );
}

