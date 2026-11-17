//! Device perf for the budgeted FX pass (`PARITY_MATRIX.md` #12 / #18).
//!
//! Runs the real `Renderer` composition (world pass + FX pass + readback) on the
//! device for several projectile/particle scenarios and writes one JSON file.
//!
//! ```text
//! cargo run -p wc3-render --release --example fx_budget -- analysis/agent-gpu/r8-fx-budget.json
//! ```
//!
//! The JSON carries the exact counters (emitted/rendered/dropped/draws/vertices/
//! upload bytes), frame-time percentiles, the frame hash and the process RSS, so
//! the budget claim is machine-checkable instead of asserted.

use std::path::PathBuf;
use std::time::Instant;

use glam::{Mat4, Vec2, Vec3};
use wc3_render::fx::{
    blizzard_wave, impact_burst, projectile_trail, FxBlend, MissileSpec, Particle, ParticleBudget,
    Rng,
};
use wc3_render::texture::TextureImage;
use wc3_render::{read_back, Counters, Renderer, Scene};

const WIDTH: u32 = 2414;
const HEIGHT: u32 = 1080;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const FRAMES: usize = 120;

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("analysis/agent-gpu/r8-fx-budget.json"));

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("no Vulkan adapter");
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("fx-budget"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("request_device");

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fx-budget-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let white = TextureImage::new(2, 2, vec![255u8; 16]);
    let atlas = TextureImage::new(2, 2, vec![255u8; 16]);
    let mut renderer = Renderer::new(
        &device,
        &queue,
        FORMAT,
        WIDTH,
        HEIGHT,
        Vec::new(),
        std::slice::from_ref(&white),
        256,
        None,
        &atlas,
        None,
    );
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let view_proj = Mat4::orthographic_lh(-10.0, 10.0, -10.0 / aspect, 10.0 / aspect, -10.0, 10.0);
    let scene = Scene::empty(view_proj, Vec2::new(WIDTH as f32, HEIGHT as f32));

    let default_budget = ParticleBudget::default();
    let mut scenarios: Vec<Scenario> = Vec::new();

    // --- empty: the pass must cost nothing when nothing emits ---
    scenarios.push(run_scenario(
        &mut renderer,
        &device,
        &queue,
        &target,
        &scene,
        "empty",
        Vec::new(),
        ParticleBudget::default(),
    ));

    // --- 64 missiles with trails and impacts (row #12) ---
    let mut rng = Rng::new(0xC0FFEE);
    let mut missiles = Vec::new();
    for index in 0..64 {
        let from = Vec3::new(rng.range(-9.0, 9.0), rng.range(-4.5, 4.5), 0.0);
        let to = Vec3::new(rng.range(-9.0, 9.0), rng.range(-4.5, 4.5), 0.0);
        let spec = MissileSpec {
            from,
            to,
            arc_height: rng.range(0.0, 4.0),
            speed: rng.range(12.0, 30.0),
            color: [0.9, 0.7, 1.0, 1.0],
            size: 0.6,
            blend: if index % 4 == 0 { FxBlend::Alpha } else { FxBlend::Additive },
        };
        // Stagger flight progress so the scene shows a spread of trail lengths.
        let time = spec.flight_time() * rng.range(0.1, 1.0);
        projectile_trail(&spec, time, 12, &mut rng, &mut missiles);
        let head = spec.from.lerp(spec.to, 0.5);
        impact_burst(head, 6, 1.2, [1.0, 0.5, 0.2, 0.8], &mut rng, &mut missiles);
    }
    scenarios.push(run_scenario(
        &mut renderer, &device, &queue, &target, &scene, "missiles_64", missiles, default_budget,
    ));

    // --- Blizzard: six waves (row #18 acceptance) ---
    let mut rng = Rng::new(0xB11_22A7D);
    let mut blizzard = Vec::new();
    for wave in 0..6 {
        blizzard_wave(Vec3::new(0.0, 0.0, 0.0), 7.0, wave, &mut rng, &mut blizzard);
        blizzard_wave(Vec3::new(6.0, 0.0, 0.0), 4.0, wave, &mut rng, &mut blizzard);
    }
    scenarios.push(run_scenario(
        &mut renderer, &device, &queue, &target, &scene, "blizzard_6w", blizzard, default_budget,
    ));

    // --- 4096-particle stress at the default budget ---
    let stress = grid(4096);
    scenarios.push(run_scenario(
        &mut renderer,
        &device,
        &queue,
        &target,
        &scene,
        "stress_4096",
        stress.clone(),
        default_budget,
    ));

    // --- the same 4096 particles under a hard 256-particle budget ---
    let capped = ParticleBudget {
        max_particles: 256,
        max_draws: 2,
        max_upload_bytes: 64 * 1024,
    };
    scenarios.push(run_scenario(
        &mut renderer, &device, &queue, &target, &scene, "budget_256_from_4096", stress, capped,
    ));

    let rss = wc3_render::perf::rss_kb();
    let peak = wc3_render::perf::peak_rss_kb();
    let json = to_json(&info, &scenarios, rss, peak);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, json) {
        Ok(()) => println!("BENCH json_written={}", path.display()),
        Err(error) => eprintln!("BENCH json_write_error={error}"),
    }
    println!(
        "BENCH memory rss_kb={rss} peak_kb={peak} ceiling_mb=1536 total_mb={:.1}",
        peak as f64 / 1024.0
    );
}

#[derive(Clone)]
struct Scenario {
    name: &'static str,
    emitted: u32,
    rendered: u32,
    dropped_budget: u32,
    draws: u32,
    vertices: u64,
    upload_bytes: u64,
    min_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    changed_pixels: u64,
    frame_hash: u64,
    max_particles: usize,
    max_upload_bytes: u64,
}

fn grid(count: usize) -> Vec<Particle> {
    let side = (count as f32).sqrt().ceil() as usize;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let x = (index % side) as f32 / side as f32 * 18.0 - 9.0;
        let y = (index / side) as f32 / side as f32 * 9.0 - 4.5;
        out.push(Particle::new(
            Vec3::new(x, y, 0.0),
            0.35,
            [0.6, 0.8, 1.0, 0.8],
            if index % 2 == 0 { FxBlend::Additive } else { FxBlend::Alpha },
        ));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn run_scenario(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    scene: &Scene,
    name: &'static str,
    particles: Vec<Particle>,
    budget: ParticleBudget,
) -> Scenario {
    renderer.fx_budget = budget;
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut samples = Vec::with_capacity(FRAMES);
    let mut last_stats = wc3_render::FxFrameStats::default();
    let mut counters = Counters::default();
    for _ in 0..FRAMES {
        renderer.set_particles(particles.clone());
        let mut frame_counters = Counters::default();
        let started = Instant::now();
        renderer.render(device, queue, &view, scene, &mut frame_counters);
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        last_stats = renderer.fx_stats();
        counters = frame_counters;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min = samples.first().copied().unwrap_or(0.0);
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];

    // One extra frame for the readback (the loop above already left a full frame).
    renderer.set_particles(particles);
    renderer.render(device, queue, &view, scene, &mut Counters::default());
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let pixels = read_back(device, queue, target, WIDTH, HEIGHT).expect("read back");
    let bg = [pixels[0], pixels[1], pixels[2]];
    let changed = pixels
        .chunks_exact(4)
        .filter(|p| p[0].abs_diff(bg[0]) > 4 || p[1].abs_diff(bg[1]) > 4 || p[2].abs_diff(bg[2]) > 4)
        .count() as u64;
    println!(
        "BENCH scenario={name} emitted={} rendered={} dropped={} draws={} vertices={} upload_bytes={} min_ms={:.3} p50_ms={:.3} p95_ms={:.3} fps={:.1} changed_pixels={} frame_hash={}",
        last_stats.emitted,
        last_stats.rendered,
        last_stats.dropped_budget,
        last_stats.draws,
        last_stats.vertices,
        last_stats.upload_bytes,
        min,
        p50,
        p95,
        1000.0 / p50.max(0.001),
        changed,
        wc3_render::frame_hash(&pixels),
    );
    println!(
        "BENCH counters scenario={name} draws={} passes={} texture_binds={} pipeline_binds={} cpu_alloc_bytes={}",
        counters.draws, counters.passes, counters.texture_binds, counters.pipeline_binds, counters.cpu_alloc_bytes
    );
    Scenario {
        name,
        emitted: last_stats.emitted,
        rendered: last_stats.rendered,
        dropped_budget: last_stats.dropped_budget,
        draws: last_stats.draws,
        vertices: last_stats.vertices,
        upload_bytes: last_stats.upload_bytes,
        min_ms: min,
        p50_ms: p50,
        p95_ms: p95,
        changed_pixels: changed,
        frame_hash: wc3_render::frame_hash(&pixels),
        max_particles: budget.max_particles,
        max_upload_bytes: budget.max_upload_bytes,
    }
}

fn to_json(info: &wgpu::AdapterInfo, scenarios: &[Scenario], rss: u64, peak: u64) -> String {
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"renderer\": \"{}\",\n", wc3_render::RENDERER));
    json.push_str(&format!("  \"width\": {WIDTH}, \"height\": {HEIGHT}, \"frames\": {FRAMES},\n"));
    json.push_str(&format!(
        "  \"adapter\": {{\"name\": {:?}, \"backend\": \"{:?}\", \"driver\": {:?}}},\n",
        info.name, info.backend, info.driver
    ));
    json.push_str("  \"scenarios\": [\n");
    for (index, s) in scenarios.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"name\": {:?}, \"emitted\": {}, \"rendered\": {}, \"dropped_budget\": {}, \"draws\": {}, \"vertices\": {}, \"upload_bytes\": {}, \"min_ms\": {:.3}, \"p50_ms\": {:.3}, \"p95_ms\": {:.3}, \"fps\": {:.1}, \"changed_pixels\": {}, \"frame_hash\": {}, \"budget_max_particles\": {}, \"budget_max_upload_bytes\": {}}}{}\n",
            s.name,
            s.emitted,
            s.rendered,
            s.dropped_budget,
            s.draws,
            s.vertices,
            s.upload_bytes,
            s.min_ms,
            s.p50_ms,
            s.p95_ms,
            1000.0 / s.p50_ms.max(0.001),
            s.changed_pixels,
            s.frame_hash,
            s.max_particles,
            s.max_upload_bytes,
            if index + 1 == scenarios.len() { "" } else { "," }
        ));
    }
    json.push_str("  ],\n");
    json.push_str(&format!(
        "  \"rss_kb\": {rss}, \"peak_kb\": {peak}, \"ceiling_mb\": 1536,\n"
    ));
    json.push_str("  \"done\": true\n}\n");
    json
}
