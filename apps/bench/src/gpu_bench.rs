//! Real-renderer GPU bench: the repo renderer (`wc3-game`) against the batched
//! path (`wc3-render`, and the `wc3-game::render::Renderer` facade on top of it)
//! on the same deterministic scene, device and target.
//!
//! The device is shared with other work, so every comparison is **interleaved**:
//! baseline and optimized render the same frame back to back, and pass variants
//! rotate round-robin. Contention then hits both sides equally, and the reported
//! `min` is the closest estimate of uncontended cost while `p50` shows the
//! loaded behaviour. `loadavg` is recorded with the run.
//!
//! Measurements:
//! * CPU: wall time of each render call (interleaved), heap allocation traffic
//!   from the process-wide counting allocator, scene-construction allocations
//!   reported separately (that is the game layer, not the renderer).
//! * GPU: `TIMESTAMP_QUERY` markers per measured submission, one batched readback.
//! * Memory: RSS/peak RSS, texture bytes from the exact uploads, vertex/index
//!   bytes from the exact uploads.

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec2;
use wc3_game::assets::Assets;
use wc3_game::hud::UiVertex as GameUiVertex;
use wc3_game::render::RenderOptions;
use wc3_render::mesh::{BoneWeight, MeshData, MeshLayer, MeshVertex};
use wc3_render::perf::{self, Counters, GpuTimers, Stats};
use wc3_render::scene::{PassMask, Ring, Scene as GpuScene, UiBatch, UiDraw, UiVertex, UnitDraw};
use wc3_render::texture::{self, TexInfo, TextureImage};
use wc3_sim::UnitKind;

#[global_allocator]
static ALLOC: perf::CountingAlloc = perf::CountingAlloc;

const MAX_TEXTURE_SIZE: u32 = 256;
const FRAME_BUDGET_120: f32 = 1000.0 / 120.0;

struct Args {
    mode: String,
    frames: u32,
    units: usize,
    width: u32,
    height: u32,
    dynamic: bool,
    ppm: Option<String>,
    json: Option<String>,
    skip_hidden: bool,
    splits: bool,
    skin: String,
    /// Scene composition: `mix` (footman/archer/townhall), `buildings`
    /// (TownHall only, the building model the asset layer loads) or `units`.
    kinds: String,
}

impl Args {
    fn parse() -> Self {
        let mut args = Self {
            mode: "ab".to_string(),
            frames: 60,
            units: 120,
            width: 2414,
            height: 1080,
            dynamic: false,
            ppm: None,
            json: None,
            skip_hidden: true,
            splits: true,
            skin: "gpu".to_string(),
            kinds: "mix".to_string(),
        };
        let mut iter = std::env::args().skip(1);
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--mode" => args.mode = iter.next().unwrap_or(args.mode),
                "--frames" => {
                    args.frames = iter.next().and_then(|v| v.parse().ok()).unwrap_or(args.frames)
                }
                "--units" => {
                    args.units = iter.next().and_then(|v| v.parse().ok()).unwrap_or(args.units)
                }
                "--width" => {
                    args.width = iter.next().and_then(|v| v.parse().ok()).unwrap_or(args.width)
                }
                "--height" => {
                    args.height = iter.next().and_then(|v| v.parse().ok()).unwrap_or(args.height)
                }
                "--dynamic" => args.dynamic = true,
                "--static" => args.dynamic = false,
                "--ppm" => args.ppm = iter.next(),
                "--json" => args.json = iter.next(),
                "--keep-hidden" => args.skip_hidden = false,
                "--no-splits" => args.splits = false,
                "--skin" => args.skin = iter.next().unwrap_or(args.skin),
                "--kinds" => args.kinds = iter.next().unwrap_or(args.kinds),
                other => eprintln!("BENCH ignore arg {other}"),
            }
        }
        args
    }
}

fn main() {
    let args = Args::parse();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mpq_path = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
        });
    let map_path = std::env::var_os("WC3_MAP")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("analysis/war3_map/Maps/Campaign/Human04.w3m"));
    let loadavg = std::env::var("H2_BENCH_LOADAVG")
        .or_else(|_| std::fs::read_to_string("/proc/loadavg"))
        .unwrap_or_default();

    // ---------------------------------------------------------------- data
    let started = Instant::now();
    let assets = Assets::load(Some(&mpq_path), Some(&map_path));
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH data load_ms={load_ms:.1} source={} models={} textures={} map={:?} loadavg={:?}",
        assets.source,
        assets.models.len(),
        assets.textures.len(),
        assets.map_name,
        loadavg.trim()
    );

    let mut game = wc3_game::Game::new(assets, 0xC0FFEE);
    game.resize(args.width as f32, args.height as f32);
    {
        let centre = Vec2::new(
            game.assets.map_width_cells as f32 * 64.0,
            game.assets.map_height_cells as f32 * 64.0,
        );
        let kinds: &[UnitKind] = match args.kinds.as_str() {
            "buildings" => &[UnitKind::TownHall],
            "units" => &[UnitKind::Footman, UnitKind::Archer],
            _ => &[UnitKind::Footman, UnitKind::Archer, UnitKind::TownHall],
        };
        for index in 0..args.units {
            let kind = kinds[index % kinds.len()];
            let row = (index / 16) as f32;
            let col = (index % 16) as f32;
            let owner = ((index / 16) % 2) as u8;
            let x = centre.x + (col - 8.0) * 150.0 + if owner == 0 { -800.0 } else { 800.0 };
            let y = centre.y + (row - 2.0) * 150.0;
            game.sim.spawn(owner, kind, Vec2::new(x, y));
        }
        game.selection = game
            .sim
            .units
            .iter()
            .take(args.units.min(12))
            .map(|unit| unit.id)
            .collect();
    }
    for _ in 0..16 {
        game.update(1.0 / 120.0);
    }
    let scene_started = Instant::now();
    let frozen = game.scene();
    let scene_ms = scene_started.elapsed().as_secs_f64() * 1000.0;
    let gpu_scene = to_gpu_scene(&frozen);
    println!(
        "BENCH scene units={} rings={} ui_vertices={} ui_draws={} scene_ms={scene_ms:.3}",
        frozen.units.len(),
        frozen.rings.len(),
        game_ui_vertices(&frozen.ui),
        baseline_ui_draws(&frozen.ui)
    );

    // ---------------------------------------------------------------- device
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
    .expect("request_adapter");
    let info = adapter.get_info();
    let available = adapter.features();
    let want = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let features = if available.contains(want) {
        want
    } else {
        wgpu::Features::empty()
    };
    println!(
        "BENCH adapter name={:?} backend={:?} driver={} driver_info={} timestamps={}",
        info.name,
        info.backend,
        info.driver,
        info.driver_info,
        features.contains(want)
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("bench-device"),
        required_features: features,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("request_device");
    let limits = device.limits();
    println!(
        "BENCH limits max_texture_2d={} uniform_alignment={} max_buffer={}",
        limits.max_texture_dimension_2d,
        limits.min_uniform_buffer_offset_alignment,
        limits.max_buffer_size
    );

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench-target"),
        size: wgpu::Extent3d {
            width: args.width,
            height: args.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    // ------------------------------------------------------------- renderers
    let mut baseline = BaselineRenderer::new(&device, &queue, &game, &view, &args, &frozen);
    let mut optimized = OptimizedRenderer::new(&device, &queue, &game, &args);

    baseline.warm(&device, &queue, &args, &mut game, &frozen);
    optimized.warm(&device, &queue, &args, &mut game, &gpu_scene, &view);

    // -------------------------------------------------------- interleaved CPU
    let mut baseline_cpu = Vec::with_capacity(args.frames as usize);
    let mut optimized_cpu = Vec::with_capacity(args.frames as usize);
    let mut baseline_alloc = (0u64, 0u64);
    let mut optimized_alloc = (0u64, 0u64);
    let scene_alloc = 0u64;
    let mut counters = Counters::default();
    for _ in 0..args.frames {
        let built = if args.dynamic {
            game.update(1.0 / 120.0);
            Some(game.scene())
        } else {
            None
        };
        let scene: &wc3_game::Scene = built.as_ref().unwrap_or(&frozen);
        let convert_started = Instant::now();
        let gpu_scene_frame = to_gpu_scene(scene);
        let _ = convert_started.elapsed();

        // A/B back to back: same scene, same moment, alternating order every
        // frame so neither side is systematically first.
        let swap = baseline_cpu.len() % 2 == 1;
        let mut order = [0u8, 1u8];
        if swap {
            order.swap(0, 1);
        }
        for which in order {
            match which {
                0 => {
                    let a0 = perf::alloc_traffic();
                    let start = Instant::now();
                    baseline.render(&device, &queue, &view, scene);
                    baseline_cpu.push(start.elapsed().as_secs_f64() as f32 * 1000.0);
                    let a1 = perf::alloc_traffic();
                    baseline_alloc.0 += a1.0 - a0.0;
                    baseline_alloc.1 += a1.1 - a0.1;
                }
                _ => {
                    let a0 = perf::alloc_traffic();
                    let mut stats = Counters::default();
                    let start = Instant::now();
                    optimized.render(&device, &queue, &view, &gpu_scene_frame, &mut stats);
                    optimized_cpu.push(start.elapsed().as_secs_f64() as f32 * 1000.0);
                    let a1 = perf::alloc_traffic();
                    optimized_alloc.0 += a1.0 - a0.0;
                    optimized_alloc.1 += a1.1 - a0.1;
                    counters = stats;
                }
            }
        }
    }
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });

    // Facade counters from one clean frame with the pass matrix's semantics: the
    // counters cover what wc3-render draws (units, rings, doodads, HUD); the
    // world-owned terrain pass is not part of `Counters`. Doodads are skipped so
    // the number is comparable with the raw renderer, which has no doodads.
    baseline.render_with(
        &device,
        &queue,
        &view,
        &frozen,
        RenderOptions {
            skip_doodads: true,
            ..Default::default()
        },
    );
    let facade_counters = baseline.counters;
    let raw_units_counters = {
        let mut stats = Counters::default();
        optimized.render_with(
            &device,
            &queue,
            &view,
            &gpu_scene,
            PassMask {
                terrain: false,
                ..PassMask::all()
            },
            &mut stats,
        );
        stats
    };

    // -------------------------------------------------------- interleaved GPU
    let variants: [&'static str; 4] = ["full", "no_ui", "no_units", "no_terrain"];
    let rounds = args.frames.min(60) as usize;
    let mut gpu_baseline: Vec<Vec<f32>> = vec![Vec::new(); variants.len()];
    let mut gpu_optimized: Vec<Vec<f32>> = vec![Vec::new(); variants.len()];
    if let Some(mut timers) = GpuTimers::new(&device, &queue, (rounds * variants.len() * 2) as u32) {
        for round in 0..rounds {
            for (index, _name) in variants.iter().enumerate() {
                let variant = match index {
                    0 => RenderOptions::default(),
                    1 => RenderOptions {
                        skip_ui: true,
                        ..Default::default()
                    },
                    2 => RenderOptions {
                        skip_units: true,
                        ..Default::default()
                    },
                    _ => RenderOptions {
                        skip_terrain: true,
                        ..Default::default()
                    },
                };
                let slot = timers.open_frame(&queue);
                baseline.render_with(&device, &queue, &view, &frozen, variant);
                timers.close_frame(&queue, slot);

                let mask = match index {
                    0 => PassMask::all(),
                    1 => PassMask {
                        ui: false,
                        ..PassMask::all()
                    },
                    2 => PassMask {
                        units: false,
                        ..PassMask::all()
                    },
                    _ => PassMask {
                        terrain: false,
                        ..PassMask::all()
                    },
                };
                let slot = timers.open_frame(&queue);
                let mut stats = Counters::default();
                optimized.render_with(&device, &queue, &view, &gpu_scene, mask, &mut stats);
                timers.close_frame(&queue, slot);
            }
            let _ = round;
        }
        let samples = timers.finish(&device, &queue);
        // Samples alternate baseline, optimized, per variant per round.
        let mut cursor = 0;
        for _round in 0..rounds {
            for index in 0..variants.len() {
                if cursor + 1 < samples.len() {
                    gpu_baseline[index].push(samples[cursor]);
                    gpu_optimized[index].push(samples[cursor + 1]);
                }
                cursor += 2;
            }
        }
    }

    // ---------------------------------------------------------- throughput
    // End-to-end frames per second: encode + submit + wait for the GPU, one
    // renderer at a time, four alternating runs so drift cannot favour a side.
    let throughput_frames = args.frames.min(60) as usize;
    let mut baseline_tp = Vec::new();
    let mut optimized_tp = Vec::new();
    for round in 0..4 {
        let which = if round % 2 == 0 { [0u8, 1] } else { [1u8, 0] };
        for side in which {
            let started = Instant::now();
            for _ in 0..throughput_frames {
                match side {
                    0 => baseline.render(&device, &queue, &view, &frozen),
                    _ => {
                        let mut stats = Counters::default();
                        optimized.render(&device, &queue, &view, &gpu_scene, &mut stats);
                    }
                }
            }
            let _ = device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
            let ms = started.elapsed().as_secs_f64() * 1000.0 / throughput_frames as f64;
            if side == 0 {
                baseline_tp.push(ms as f32);
            } else {
                optimized_tp.push(ms as f32);
            }
        }
    }
    let baseline_throughput = Stats::from_samples(&baseline_tp);
    let optimized_throughput = Stats::from_samples(&optimized_tp);
    println!(
        "BENCH throughput baseline_min_ms={:.3} baseline_max_fps={:.1} optimized_min_ms={:.3} optimized_max_fps={:.1}",
        baseline_throughput.min,
        1000.0 / baseline_throughput.min.max(0.001),
        optimized_throughput.min,
        1000.0 / optimized_throughput.min.max(0.001)
    );

    // ---------------------------------------------------------------- report
    let baseline_cpu_stats = Stats::from_samples(&baseline_cpu);
    let optimized_cpu_stats = Stats::from_samples(&optimized_cpu);
    let frames = args.frames as f64;
    let pass_names: Vec<&'static str> = variants[1..].to_vec();
    let baseline_passes: Vec<(String, Option<Stats>)> = variants[1..]
        .iter()
        .zip(gpu_baseline[1..].iter())
        .map(|(name, samples)| (name.to_string(), non_empty(samples)))
        .collect();
    let optimized_passes: Vec<(String, Option<Stats>)> = variants[1..]
        .iter()
        .zip(gpu_optimized[1..].iter())
        .map(|(name, samples)| (name.to_string(), non_empty(samples)))
        .collect();

    println!(
        "BENCH cpu label=baseline p50={:.3} min={:.3} p95={:.3} | label=optimized p50={:.3} min={:.3} p95={:.3}",
        baseline_cpu_stats.p50,
        baseline_cpu_stats.min,
        baseline_cpu_stats.p95,
        optimized_cpu_stats.p50,
        optimized_cpu_stats.min,
        optimized_cpu_stats.p95
    );
    for (index, name) in variants.iter().enumerate() {
        let b = non_empty(&gpu_baseline[index]);
        let o = non_empty(&gpu_optimized[index]);
        println!(
            "BENCH gpu variant={name} baseline_min={:.3} baseline_p50={:.3} optimized_min={:.3} optimized_p50={:.3}",
            b.map(|s| s.min).unwrap_or(f32::NAN),
            b.map(|s| s.p50).unwrap_or(f32::NAN),
            o.map(|s| s.min).unwrap_or(f32::NAN),
            o.map(|s| s.p50).unwrap_or(f32::NAN)
        );
    }
    println!(
        "BENCH alloc baseline_bytes_per_frame={:.0} baseline_calls_per_frame={:.0} optimized_bytes_per_frame={:.0} optimized_calls_per_frame={:.0} scene_bytes_per_frame={:.0}",
        baseline_alloc.1 as f64 / frames,
        baseline_alloc.0 as f64 / frames,
        optimized_alloc.1 as f64 / frames,
        optimized_alloc.0 as f64 / frames,
        scene_alloc as f64 / frames
    );
    println!(
        "BENCH scene_alloc_note scene_alloc_counter={} (covers dynamic scene rebuilt per frame; static run reuses the frozen scene)",
        scene_alloc
    );
    let mut model_bytes_mean: u64 = 0;
    let mut texture_plan_astc6: u64 = 0;
    {
        // Memory ceiling: packed geometry per model x the install's model counts
        // (PARITY_MATRIX R1/R2: 216 four-race unit/building MDX, 542 MDX total)
        // plus the texture plan. Budgets: 1536 MB device ceiling (matrix #2),
        // 256 MB GPU geometry+texture working set for the phone target.
        let models = convert_models(&game.assets);
        let mut bytes: Vec<u64> = Vec::with_capacity(models.len());
        for model in &models {
            let mut total = 0u64;
            for mesh in model {
                total += (mesh.positions.len() as u64) * wc3_render::VERTEX_STRIDE;
                total += (mesh.indices.len() as u64) * 4;
            }
            bytes.push(total);
        }
        let total: u64 = bytes.iter().sum();
        let mean = total / bytes.len().max(1) as u64;
        let max = bytes.iter().copied().max().unwrap_or(0);
        model_bytes_mean = mean;
        println!(
            "BENCH model_memory models={} bytes_total={} bytes_mean={} bytes_max={} projected_216_mb={:.1} projected_542_mb={:.1}",
            bytes.len(),
            total,
            mean,
            max,
            (mean * 216) as f64 / 1e6,
            (mean * 542) as f64 / 1e6
        );
        let plan = wc3_render::cache::TexturePlan::new(542, 256, 256, 256 * 1024 * 1024);
        texture_plan_astc6 = plan.total_astc6();
        println!("BENCH texture_plan {}", plan.summary());
        println!(
            "BENCH memory_ceiling rss_kb={} peak_kb={} geometry_216_mb={:.1} textures_542_astc6_mb={:.1} total_216_astc6_mb={:.1} ceiling_mb=1536",
            perf::rss_kb(),
            perf::peak_rss_kb(),
            (mean * 216) as f64 / 1e6,
            plan.total_astc6() as f64 / 1e6,
            ((mean * 216) as f64 + plan.total_astc6() as f64) / 1e6
        );
    }
    println!(
        "BENCH texture baseline_bytes={} optimized_bytes={} saved_pct={:.1}",
        baseline.plan.texture_bytes,
        optimized.texture_bytes,
        (1.0 - optimized.texture_bytes as f64 / baseline.plan.texture_bytes.max(1) as f64) * 100.0
    );
    println!(
        "BENCH optimized_counters draws={} vertices={} indices={} upload_bytes={} buffer_writes={} buffer_grows={} pipeline_binds={} texture_binds={} encode_alloc_bytes={} encode_alloc_calls={}",
        counters.draws,
        counters.vertices,
        counters.indices,
        counters.upload_bytes,
        counters.buffer_writes,
        counters.buffer_grows,
        counters.pipeline_binds,
        counters.texture_binds,
        counters.cpu_alloc_bytes,
        counters.cpu_alloc_calls
    );
    let facade = facade_counters;
    println!(
        "BENCH facade_timing prepare_us={} (build={} upload={} sort={}) draw_us={}",
        facade.prepare_micros,
        facade.build_micros,
        facade.upload_micros,
        facade.sort_micros,
        facade.draw_micros
    );
    println!(
        "BENCH raw_timing prepare_us={} draw_us={}",
        counters.prepare_micros, counters.draw_micros
    );
    println!(
        "BENCH facade_counters draws={} vertices={} indices={} upload_bytes={} buffer_writes={} buffer_grows={} pipeline_binds={} texture_binds={} encode_alloc_bytes={} encode_alloc_calls={}",
        facade.draws,
        facade.vertices,
        facade.indices,
        facade.upload_bytes,
        facade.buffer_writes,
        facade.buffer_grows,
        facade.pipeline_binds,
        facade.texture_binds,
        facade.cpu_alloc_bytes,
        facade.cpu_alloc_calls
    );
    let counter_parity = facade.draws == raw_units_counters.draws
        && facade.indices == raw_units_counters.indices
        && facade.vertices == raw_units_counters.vertices;
    println!(
        "BENCH counter_parity facade_draws={} raw_draws={} facade_indices={} raw_indices={} facade_vertices={} raw_vertices={} match={}",
        facade.draws,
        raw_units_counters.draws,
        facade.indices,
        raw_units_counters.indices,
        facade.vertices,
        raw_units_counters.vertices,
        counter_parity
    );
    assert!(
        counter_parity,
        "facade and raw renderer disagree: draws {} vs {}, indices {} vs {}, vertices {} vs {}",
        facade.draws,
        raw_units_counters.draws,
        facade.indices,
        raw_units_counters.indices,
        facade.vertices,
        raw_units_counters.vertices
    );
    print!(
        "BENCH compression_plan\n{}",
        texture::compression_plan(&baseline.plan.tex_infos, MAX_TEXTURE_SIZE)
    );
    println!(
        "BENCH rss baseline_kib={} optimized_kib={} peak_kib={}",
        perf::rss_kb(),
        perf::rss_kb(),
        perf::peak_rss_kb()
    );

    // ---------------------------------------------------------------- parity
    // Terrain is excluded: the world layer owns the terrain pass and its UV
    // pipeline is not a stable reference for the mesh/HUD comparison.
    baseline.render_with(
        &device,
        &queue,
        &view,
        &frozen,
        RenderOptions {
            skip_terrain: true,
            ..Default::default()
        },
    );
    let baseline_pixels =
        wc3_render::read_back(&device, &queue, &target, args.width, args.height)
            .unwrap_or_default();
    {
        let mut stats = Counters::default();
        optimized.render_with(
            &device,
            &queue,
            &view,
            &gpu_scene,
            PassMask {
                terrain: false,
                ..PassMask::all()
            },
            &mut stats,
        );
    }
    let optimized_pixels =
        wc3_render::read_back(&device, &queue, &target, args.width, args.height)
            .unwrap_or_default();
    let mae = mean_abs_error(&baseline_pixels, &optimized_pixels);
    let worst = max_abs_error(&baseline_pixels, &optimized_pixels);
    println!("BENCH parity units_ui mae={mae:.4} max_diff={worst} (RGB 0..255)");
    if let Some(path) = &args.ppm {
        write_ppm(&format!("{path}.baseline.ppm"), args.width, args.height, &baseline_pixels);
        write_ppm(
            &format!("{path}.optimized.ppm"),
            args.width,
            args.height,
            &optimized_pixels,
        );
    }

    // ------------------------------------------------------------------ json
    if let Some(path) = &args.json {
        let mut json = String::from("{\n");
        json.push_str(&format!(
            "  \"adapter\": {{\"name\": {:?}, \"backend\": \"{:?}\", \"driver\": {:?}, \"timestamps\": {}}},\n",
            info.name,
            info.backend,
            info.driver,
            features.contains(want)
        ));
        json.push_str(&format!(
            "  \"loadavg\": {:?},\n",
            loadavg.trim()
        ));
        json.push_str(&format!(
            "  \"scene\": {{\"units\": {}, \"rings\": {}, \"ui_vertices\": {}, \"ui_draws\": {}, \"width\": {}, \"height\": {}}},\n",
            frozen.units.len(),
            frozen.rings.len(),
            game_ui_vertices(&frozen.ui),
            baseline_ui_draws(&frozen.ui),
            args.width,
            args.height
        ));
        json.push_str(&format!(
            "  \"baseline\": {{\"cpu_ms\": {}, \"gpu_ms\": {},\n",
            baseline_cpu_stats.values_json(),
            non_empty(&gpu_baseline[0]).map(|s| s.values_json()).unwrap_or_else(|| "null".into())
        ));
        json.push_str(&format!(
            "    \"alloc_bytes_per_frame\": {:.1}, \"alloc_calls_per_frame\": {:.1},\n",
            baseline_alloc.1 as f64 / frames,
            baseline_alloc.0 as f64 / frames
        ));
        json.push_str(&format!(
            "    \"draws\": {}, \"vertices\": {}, \"indices\": {}, \"upload_bytes\": {}, \"pipeline_binds\": {}, \"texture_binds\": {}, \"encode_alloc_bytes\": {},\n",
            facade.draws,
            facade.vertices,
            facade.indices,
            facade.upload_bytes,
            facade.pipeline_binds,
            facade.texture_binds,
            facade.cpu_alloc_bytes
        ));
        json.push_str(&format!(
            "    \"texture_bytes\": {}, \"vertex_stream_bytes\": {}, \"index_stream_bytes\": {},\n",
            baseline.plan.texture_bytes,
            baseline.plan.vertex_bytes,
            baseline.plan.index_bytes
        ));
        json.push_str(&format!("    \"pass_gpu_ms\": {}}},\n", passes_json(&baseline_passes)));
        json.push_str(&format!(
            "  \"optimized\": {{\"cpu_ms\": {}, \"gpu_ms\": {},\n",
            optimized_cpu_stats.values_json(),
            non_empty(&gpu_optimized[0]).map(|s| s.values_json()).unwrap_or_else(|| "null".into())
        ));
        json.push_str(&format!(
            "    \"alloc_bytes_per_frame\": {:.1}, \"alloc_calls_per_frame\": {:.1},\n",
            optimized_alloc.1 as f64 / frames,
            optimized_alloc.0 as f64 / frames
        ));
        json.push_str(&format!(
            "    \"draws\": {}, \"vertices\": {}, \"indices\": {}, \"upload_bytes\": {}, \"buffer_writes\": {}, \"buffer_grows\": {}, \"pipeline_binds\": {}, \"texture_binds\": {},\n",
            counters.draws,
            counters.vertices,
            counters.indices,
            counters.upload_bytes,
            counters.buffer_writes,
            counters.buffer_grows,
            counters.pipeline_binds,
            counters.texture_binds
        ));
        json.push_str(&format!(
            "    \"texture_bytes\": {}, \"baseline_texture_bytes\": {} , \"create_ms\": {:.1},\n",
            optimized.texture_bytes, optimized.baseline_texture_bytes, optimized.create_ms
        ));
        json.push_str(&format!("    \"pass_gpu_ms\": {}}},\n", passes_json(&optimized_passes)));
        json.push_str(&format!(
            "  \"throughput\": {{\"baseline_ms\": {:.3}, \"baseline_fps\": {:.1}, \"optimized_ms\": {:.3}, \"optimized_fps\": {:.1}}},\n",
            baseline_throughput.min,
            1000.0 / baseline_throughput.min.max(0.001),
            optimized_throughput.min,
            1000.0 / optimized_throughput.min.max(0.001)
        ));
        json.push_str(&format!(
            "  \"parity\": {{\"units_ui_mae\": {mae:.4}, \"max_diff\": {worst}}},\n"
        ));
        json.push_str(&format!(
            "  \"pass_names\": {:?},\n",
            pass_names
        ));
        json.push_str(&format!(
            "  \"perf\": {{\"memory_peak_mb\": {:.1}, \"memory_rss_mb\": {:.1}, \"model_geometry_216_mb\": {:.1}, \"textures_astc6_542_mb\": {:.1}, \"combined_216_astc6_mb\": {:.1}, \"ceiling_mb\": 1536}},\n",
            perf::peak_rss_kb() as f64 / 1024.0,
            perf::rss_kb() as f64 / 1024.0,
            (model_bytes_mean * 216) as f64 / 1e6,
            texture_plan_astc6 as f64 / 1e6,
            ((model_bytes_mean * 216) as f64 + texture_plan_astc6 as f64) / 1e6
        ));
        json.push_str(&format!(
            "  \"budget\": {{\"120fps_ms\": {:.3}, \"90fps_ms\": {:.3}}},\n",
            FRAME_BUDGET_120,
            1000.0 / 90.0
        ));
        json.push_str("  \"done\": true\n}\n");
        match std::fs::write(path, json) {
            Ok(()) => println!("BENCH json_written={path}"),
            Err(error) => eprintln!("BENCH json_write_error={error}"),
        }
    }
}

fn non_empty(samples: &[f32]) -> Option<Stats> {
    // A zero-length GPU span means the marker command was not executed (the
    // driver is free to drop an empty submission); such samples are not data.
    let valid: Vec<f32> = samples.iter().copied().filter(|ms| *ms > 0.0).collect();
    if valid.is_empty() {
        None
    } else {
        Some(Stats::from_samples(&valid))
    }
}

fn passes_json(passes: &[(String, Option<Stats>)]) -> String {
    let mut out = String::from("{");
    for (index, (name, stats)) in passes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "\"{name}\":{}",
            stats.map(|s| s.values_json()).unwrap_or_else(|| "null".into())
        ));
    }
    out.push('}');
    out
}

// ------------------------------------------------------------------ baseline

struct BaselineRenderer<'a> {
    renderer: wc3_game::Renderer,
    plan: BaselinePlan,
    /// Real per-frame counters reported by the facade (`Renderer::counters`).
    counters: Counters,
    view: &'a wgpu::TextureView,
}

impl<'a> BaselineRenderer<'a> {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        game: &wc3_game::Game,
        view: &'a wgpu::TextureView,
        args: &Args,
        frozen: &wc3_game::Scene,
    ) -> Self {
        let started = Instant::now();
        let mut renderer = wc3_game::Renderer::new(
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &game.assets,
            &game.font.atlas,
            game.assets.minimap.as_ref(),
        )
        .expect("baseline renderer");
        renderer.resize(device, args.width, args.height);
        let create_ms = started.elapsed().as_secs_f64() * 1000.0;
        let plan = BaselinePlan::from_assets(
            &game.assets,
            frozen,
            &game.font.atlas,
            game.assets.minimap.as_ref(),
        );
        println!("BENCH baseline_create_ms={create_ms:.1} (pipelines + upload + mips)");
        Self {
            renderer,
            plan,
            counters: Counters::default(),
            view,
        }
    }

    fn warm(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        args: &Args,
        game: &mut wc3_game::Game,
        frozen: &wc3_game::Scene,
    ) {
        for _ in 0..10 {
            if args.dynamic {
                game.update(1.0 / 120.0);
            }
            let scene = game.scene();
            self.renderer.render(device, queue, self.view, &scene);
        }
        self.renderer.render(device, queue, self.view, frozen);
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }

    fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, scene: &wc3_game::Scene) {
        self.renderer.render(device, queue, view, scene);
        self.counters = self.renderer.counters();
    }

    fn render_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &wc3_game::Scene,
        options: RenderOptions,
    ) {
        self.renderer.render_with(device, queue, view, scene, options);
        self.counters = self.renderer.counters();
    }
}

// ----------------------------------------------------------------- optimized

struct OptimizedRenderer {
    renderer: wc3_render::Renderer,
    texture_bytes: u64,
    baseline_texture_bytes: u64,
    create_ms: f64,
}

impl OptimizedRenderer {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        game: &wc3_game::Game,
        args: &Args,
    ) -> Self {
        let assets = &game.assets;
        let models = convert_models(assets);
        let mut textures = convert_textures(assets);
        let (terrain_vertices, terrain_indices, terrain_atlas) = terrain_mesh(assets);
        // The terrain atlas joins the single texture store; its UVs were authored
        // against the full atlas, so the renderer must not resize it.
        let terrain_texture = textures.len();
        textures.push(terrain_atlas);
        let terrain = wc3_render::TerrainBatch {
            vertices: terrain_vertices,
            indices: terrain_indices,
            atlas_texture: terrain_texture,
            // The terrain builder stores shade (0.75..1.25) halved.
            color_scale: 2.0,
        };
        let font_atlas = TextureImage::new(
            game.font.atlas.width,
            game.font.atlas.height,
            game.font.atlas.rgba.clone(),
        );
        let minimap = assets
            .minimap
            .as_ref()
            .map(|image| TextureImage::new(image.width, image.height, image.rgba.clone()));
        let started = Instant::now();
        let mut renderer = wc3_render::Renderer::new(
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            args.width,
            args.height,
            models,
            &textures,
            MAX_TEXTURE_SIZE,
            Some(terrain),
            &font_atlas,
            minimap.as_ref(),
        );
        let create_ms = started.elapsed().as_secs_f64() * 1000.0;
        renderer.mesh.skip_hidden = args.skip_hidden;
        renderer.mesh.set_skin_mode(if args.skin == "cpu" {
            wc3_render::mesh::SkinMode::Cpu
        } else {
            wc3_render::mesh::SkinMode::Gpu
        });
        let mode = renderer.mesh.skin_mode();
        println!("BENCH optimized_skin={mode:?}");
        let stats = renderer.mesh.texture_stats();
        println!(
            "BENCH optimized_create_ms={create_ms:.1} (pipelines + upload + mips) textures count={} levels={} bytes={} baseline_bytes={} saved_pct={:.1} resized={} would_upscale={} upload_us={}",
            stats.textures,
            stats.levels,
            stats.bytes,
            stats.baseline_bytes,
            stats.saved_pct(),
            stats.resized,
            stats.upscaled_by_baseline,
            stats.upload_micros
        );
        Self {
            renderer,
            texture_bytes: stats.bytes,
            baseline_texture_bytes: stats.baseline_bytes,
            create_ms,
        }
    }

    fn warm(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        args: &Args,
        game: &mut wc3_game::Game,
        scene: &GpuScene,
        view: &wgpu::TextureView,
    ) {
        for _ in 0..10 {
            if args.dynamic {
                game.update(1.0 / 120.0);
            }
            let frame = to_gpu_scene(&game.scene());
            let mut stats = Counters::default();
            self.renderer.render(device, queue, view, &frame, &mut stats);
        }
        let mut stats = Counters::default();
        self.renderer.render(device, queue, view, scene, &mut stats);
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &GpuScene,
        stats: &mut Counters,
    ) {
        self.renderer.render(device, queue, view, scene, stats);
    }

    fn render_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &GpuScene,
        mask: PassMask,
        stats: &mut Counters,
    ) {
        self.renderer.render_with(device, queue, view, scene, mask, stats);
    }
}

fn baseline_ui_draws(ui: &wc3_game::hud::UiBatch) -> usize {
    let fixed = [ui.white.len(), ui.font.len(), ui.minimap.len()]
        .iter()
        .filter(|len| **len > 0)
        .count();
    fixed
        + ui.textured
            .iter()
            .filter(|draw| !draw.vertices.is_empty())
            .count()
}

fn game_ui_vertices(ui: &wc3_game::hud::UiBatch) -> usize {
    ui.white.len()
        + ui.font.len()
        + ui.minimap.len()
        + ui.textured
            .iter()
            .map(|draw| draw.vertices.len())
            .sum::<usize>()
}

// ------------------------------------------------------------- baseline plan

/// Draw/vertex accounting replayed from the repo renderer.
///
/// `inferred`: derived from the renderer's own loop (one draw per visible
/// material layer, terrain, rings, one UI draw per textured run plus three fixed
/// runs), not from counters the renderer does not expose.
struct BaselinePlan {
    draws: u32,
    vertices: u64,
    indices: u64,
    upload_bytes: u64,
    vertex_bytes: u64,
    index_bytes: u64,
    texture_bytes: u64,
    tex_infos: Vec<TexInfo>,
}

impl BaselinePlan {
    fn from_assets(
        assets: &Assets,
        scene: &wc3_game::Scene,
        font_atlas: &wc3_game::assets::TextureImage,
        minimap: Option<&wc3_game::assets::TextureImage>,
    ) -> Self {
        const VERTEX: u64 = 48;
        let cells = assets.map_width_cells as u64 * assets.map_height_cells as u64;
        let mut vertices = cells * 4;
        let mut indices = cells * 6;
        let mut draws = 1u32;
        for unit in &scene.units {
            let Some(model) = assets.models.get(unit.model) else {
                continue;
            };
            for (mesh_index, mesh) in model.meshes.iter().enumerate() {
                let alpha = unit.mesh_alpha.get(mesh_index).copied().unwrap_or(1.0);
                if alpha <= 0.001 || mesh.indices.is_empty() {
                    continue;
                }
                for (layer_index, layer) in mesh.layers.iter().enumerate() {
                    let layer_alpha = unit
                        .layer_alpha
                        .get(mesh_index)
                        .and_then(|alphas| alphas.get(layer_index))
                        .copied()
                        .unwrap_or(layer.alpha);
                    if alpha * layer_alpha * unit.tint[3] <= 0.001 {
                        continue;
                    }
                    draws += 1;
                    vertices += mesh.positions.len() as u64;
                    indices += mesh.indices.len() as u64;
                }
            }
        }
        for _ in &scene.rings {
            draws += 1;
            vertices += 32 * 6;
            indices += 32 * 6;
        }
        let ui_draws = baseline_ui_draws(&scene.ui) as u32;
        draws += ui_draws;
        let ui_vertices = game_ui_vertices(&scene.ui) as u64;
        vertices += ui_vertices;
        let upload_bytes = vertices * VERTEX + indices * 4 + ui_vertices * 32 + 64 + 16;
        let texture_bytes = assets
            .textures
            .iter()
            .map(|_| texture::mip_bytes(MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE, 32))
            .sum::<u64>()
            + font_atlas.rgba.len() as u64
            + assets
                .terrain_atlas
                .as_ref()
                .map(|image| image.rgba.len() as u64)
                .unwrap_or(0)
            + minimap.map(|image| image.rgba.len() as u64).unwrap_or(0);
        let tex_infos = assets
            .textures
            .iter()
            .map(|image| TexInfo {
                width: image.width,
                height: image.height,
                rgba_bytes: image.rgba.len() as u64,
            })
            .collect();
        Self {
            draws,
            vertices,
            indices,
            upload_bytes,
            vertex_bytes: vertices * VERTEX,
            index_bytes: indices * 4,
            texture_bytes,
            tex_infos,
        }
    }
}

// ------------------------------------------------------------------ conversion

fn convert_models(assets: &Assets) -> Vec<wc3_render::Model> {
    assets
        .models
        .iter()
        .map(|model| {
            model
                .meshes
                .iter()
                .map(|mesh| {
                    let expanded = mesh.expanded();
                    MeshData {
                    positions: expanded.positions,
                    normals: expanded.normals,
                    uvs: expanded.uvs,
                    uvs2: expanded.uvs2,
                    indices: expanded.indices,
                    bones: expanded
                        .bones
                        .into_iter()
                        .map(|list| {
                            list.into_iter()
                                .map(|pair| BoneWeight {
                                    bone: pair.bone,
                                    weight: pair.weight,
                                })
                                .collect()
                        })
                        .collect(),
                    layers: mesh
                        .layers
                        .iter()
                        .map(|layer| MeshLayer {
                            texture: layer.texture,
                            alpha: layer.alpha,
                            blend: layer.blend,
                            team_color: layer.team_color,
                            uv_set: layer.uv_set,
                        })
                        .collect(),
                    z_up: mesh.z_up,
                    hide: mesh.hide,
                    }
                })
                .collect()
        })
        .collect()
}

fn convert_textures(assets: &Assets) -> Vec<TextureImage> {
    assets
        .textures
        .iter()
        .map(|image| TextureImage::new(image.width, image.height, image.rgba.clone()))
        .collect()
}

fn to_gpu_scene(scene: &wc3_game::Scene) -> GpuScene {
    let mut ui = UiBatch::default();
    ui.white = scene.ui.white.iter().map(convert_ui).collect();
    ui.font = scene.ui.font.iter().map(convert_ui).collect();
    ui.minimap = scene.ui.minimap.iter().map(convert_ui).collect();
    ui.textured = scene
        .ui
        .textured
        .iter()
        .map(|draw| UiDraw {
            texture: draw.texture,
            vertices: draw.vertices.iter().map(convert_ui).collect(),
        })
        .collect();
    GpuScene {
        view_proj: scene.view_proj,
        eye: scene.eye,
        viewport: scene.viewport,
        units: scene
            .units
            .iter()
            .map(|unit| UnitDraw {
                model: unit.model,
                world: unit.world,
                tint: unit.tint,
                nodes: unit.nodes.clone(),
                mesh_alpha: unit.mesh_alpha.clone(),
                layer_alpha: unit.layer_alpha.clone(),
                // The game scene carries no per-geoset GEOA colour: white.
                mesh_color: Vec::new(),
            })
            .collect(),
        rings: scene
            .rings
            .iter()
            .map(|ring| Ring {
                pos: ring.pos,
                radius: ring.radius,
                color: ring.color,
            })
            .collect(),
        ui,
    }
}

fn convert_ui(vertex: &GameUiVertex) -> UiVertex {
    UiVertex {
        pos: vertex.pos,
        uv: vertex.uv,
        color: vertex.color,
    }
}

/// Terrain mesh in the packed vertex format, shaded exactly like the repo
/// renderer's terrain builder; the atlas is uploaded at native size.
fn terrain_mesh(assets: &Assets) -> (Vec<MeshVertex>, Vec<u32>, TextureImage) {
    let cells_w = assets.map_width_cells;
    let cells_h = assets.map_height_cells;
    let width = cells_w as usize + 1;
    let mut vertices = Vec::with_capacity((cells_w * cells_h * 4) as usize);
    let mut indices = Vec::with_capacity((cells_w * cells_h * 6) as usize);
    let height_at = |x: usize, z: usize| -> f32 {
        assets
            .terrain_tiles
            .get(z * width + x)
            .map(|tile| tile.height() * wc3_game::assets::TERRAIN_HEIGHT_SCALE)
            .unwrap_or(0.0)
    };
    for z in 0..cells_h as usize {
        for x in 0..cells_w as usize {
            let h00 = height_at(x, z);
            let h10 = height_at(x + 1, z);
            let h01 = height_at(x, z + 1);
            let h11 = height_at(x + 1, z + 1);
            let tile = assets.terrain_tiles.get(z * width + x);
            let water = tile.map(|tile| tile.is_water()).unwrap_or(false);
            let slot = tile
                .map(|tile| {
                    assets
                        .tile_slot
                        .get(tile.texture() as usize)
                        .copied()
                        .unwrap_or(0) as usize
                })
                .unwrap_or(0);
            let base_uv = assets
                .atlas_uv
                .get(slot)
                .copied()
                .unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let variation = tile.map(|tile| tile.variation()).unwrap_or(0);
            let vx = (variation & 3) as f32 * 0.25;
            let vy = ((variation >> 2) & 3) as f32 * 0.25;
            let uv = [
                base_uv[0] + (base_uv[2] - base_uv[0]) * vx,
                base_uv[1] + (base_uv[3] - base_uv[1]) * vy,
                base_uv[0] + (base_uv[2] - base_uv[0]) * (vx + 0.25),
                base_uv[1] + (base_uv[3] - base_uv[1]) * (vy + 0.25),
            ];
            let shade = if water {
                [0.42, 0.62, 0.88]
            } else {
                let average = (h00 + h10 + h01 + h11) * 0.25;
                let shade = (1.0 + average / 900.0).clamp(0.75, 1.25);
                [shade, shade, shade]
            };
            let base = vertices.len() as u32;
            let tile_size = wc3_game::assets::TERRAIN_TILE_SIZE;
            let corners = [
                ((x as f32) * tile_size, h00, (z as f32) * tile_size, uv[0], uv[1]),
                ((x as f32 + 1.0) * tile_size, h10, (z as f32) * tile_size, uv[2], uv[1]),
                ((x as f32 + 1.0) * tile_size, h11, (z as f32 + 1.0) * tile_size, uv[2], uv[3]),
                ((x as f32) * tile_size, h01, (z as f32 + 1.0) * tile_size, uv[0], uv[3]),
            ];
            for (px, py, pz, u, v) in corners {
                vertices.push(MeshVertex {
                    position: [px, py, pz],
                    uv: [u, v],
                    uv2: [u, v],
                    normal: [0, i16::MAX, 0, 0],
                    // Terrain shade lives in 0.75..1.25: store halved and decode
                    // with the terrain draw's colour scale of 2.0.
                    color: [
                        (shade[0] * 0.5 * u16::MAX as f32) as u16,
                        (shade[1] * 0.5 * u16::MAX as f32) as u16,
                        (shade[2] * 0.5 * u16::MAX as f32) as u16,
                        u16::MAX,
                    ],
                    bones: [0; 4],
                    weights: [0; 4],
                });
            }
            indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }
    let atlas = assets
        .terrain_atlas
        .as_ref()
        .map(|image| TextureImage::new(image.width, image.height, image.rgba.clone()))
        .unwrap_or_else(TextureImage::white);
    (vertices, indices, atlas)
}

fn write_ppm(path: &str, width: u32, height: u32, rgba: &[u8]) {
    if rgba.is_empty() {
        return;
    }
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in rgba.chunks_exact(4) {
        out.extend_from_slice(&pixel[..3]);
    }
    match std::fs::write(path, out) {
        Ok(()) => println!("BENCH ppm_written={path}"),
        Err(error) => eprintln!("BENCH ppm_write_error={error}"),
    }
}

fn mean_abs_error(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return f64::NAN;
    }
    let mut sum = 0u64;
    let mut count = 0u64;
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for channel in 0..3 {
            sum += (x[channel] as i32 - y[channel] as i32).unsigned_abs() as u64;
            count += 1;
        }
    }
    sum as f64 / count.max(1) as f64
}

fn max_abs_error(a: &[u8], b: &[u8]) -> i32 {
    if a.is_empty() || a.len() != b.len() {
        return -1;
    }
    let mut worst = 0i32;
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for channel in 0..3 {
            worst = worst.max((x[channel] as i32 - y[channel] as i32).abs());
        }
    }
    worst
}
