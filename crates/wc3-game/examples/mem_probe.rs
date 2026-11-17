//! Diagnostic: CPU-side memory attribution per asset layer (the app's native
//! heap should be a few hundred MB, not a gigabyte).
//!
//! Reports, per layer, the bytes the process actually holds, plus the process
//! RSS/VmHWM at every stage, so a regression shows up as a number rather than
//! as "the phone got hot". `--budget` makes it exit non-zero when a layer
//! exceeds the budget in `analysis/h2/MEMORY_BUDGET.md`.
use std::path::Path;
use wc3_game::assets::Assets;

/// Resident set size in KiB from `/proc/self/statm` (pages).
fn rss_kb() -> u64 {
    let text = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: u64 = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    pages * (4096 / 1024)
}

fn hwm_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.trim().trim_end_matches(" kB").parse().ok())
        .unwrap_or(0)
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

struct Stage {
    name: &'static str,
    rss: u64,
}

fn main() {
    let budget = std::env::args().any(|arg| arg == "--budget");
    if std::env::args().any(|arg| arg == "--breakdown") {
        breakdown();
        return;
    }
    if std::env::args().any(|arg| arg == "--scope") {
        scope_report();
        return;
    }
    if std::env::args().any(|arg| arg == "--frame") {
        frame_report();
        return;
    }
    // `H2_MEM_INSTALL` lets the gate pass the install root explicitly (the
    // device path, or the CI copy under $PREFIX/tmp).
    let install = std::env::var_os("H2_MEM_INSTALL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new("/data/data/org.wc3rust.p0/files").to_path_buf());
    let root = install.as_path();
    // `H2_MEM_MAP` lets a host pair an archive copy with the repo's map (the CI
    // mirror keeps only the archives).
    let map = std::env::var_os("H2_MEM_MAP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("Melee.w3x"));
    if !map.is_file() {
        println!("skip: {map:?} missing");
        return;
    }
    let mut stages = vec![Stage {
        name: "start",
        rss: rss_kb(),
    }];
    let started = std::time::Instant::now();
    let mut assets = Assets::load(Some(&root.join("War3.mpq")), Some(&map));
    let load_ms = started.elapsed().as_millis();
    stages.push(Stage {
        name: "assets",
        rss: rss_kb(),
    });

    // ---- textures: every RGBA the asset layer keeps resident ---------------
    let texture_bytes: u64 = assets
        .textures
        .iter()
        .map(|image| image.rgba.len() as u64)
        .sum();
    let terrain_tile_bytes: u64 = assets
        .terrain_tile_images
        .iter()
        .map(|image| image.rgba.len() as u64)
        .sum();
    let atlas_bytes = assets
        .terrain_atlas
        .as_ref()
        .map(|image| image.rgba.len() as u64)
        .unwrap_or(0);
    let minimap_bytes = assets
        .minimap
        .as_ref()
        .map(|image| image.rgba.len() as u64)
        .unwrap_or(0);

    let mut sizes: Vec<(usize, usize)> = assets
        .textures
        .iter()
        .enumerate()
        .map(|(index, image)| (index, image.rgba.len()))
        .collect();
    sizes.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    let top: Vec<String> = sizes
        .iter()
        .take(8)
        .map(|(index, bytes)| format!("#{index}:{}x{}={}kB", assets.textures[*index].width, assets.textures[*index].height, bytes / 1024))
        .collect();

    // ---- meshes: the runtime vertex data (compact format) ---------------
    let mut verts = 0u64;
    let mut mesh_count = 0u64;
    let mut mesh_layers = 0u64;
    let mut triangles = 0u64;
    for model in &assets.models {
        // Placeholders are a single box each; they are counted in
        // `placeholders=` and would only blur the real geometry numbers.
        if model.fallback {
            continue;
        }
        for mesh in &model.meshes {
            mesh_count += 1;
            verts += mesh.vertex_count() as u64;
            triangles += mesh.triangle_count() as u64;
            mesh_layers += mesh.layers.len() as u64;
        }
    }
    let mesh_bytes = assets.mesh_bytes() as u64;
    let bytes_per_vertex = if verts > 0 {
        mesh_bytes as f64 / verts as f64
    } else {
        0.0
    };
    // The runtime format after `Assets::load` compacts: record size per vertex
    // and the whole mesh CPU (record + index buffer + metadata).
    let vertex_record = assets.vertex_record_bytes();
    let cpu_bytes_per_vertex = assets.bytes_per_vertex();

    // ---- MDX geosets retained next to the meshes ---------------------------
    let mut geoset_bytes = 0u64;
    let mut mdx_models = 0u64;
    let mut mdx_bones = 0u64;
    let mut mdx_sequences = 0u64;
    let mut raw_chunk_bytes = 0u64;
    let mut raw_chunk_count = 0u64;
    let mut node_count = 0u64;
    let mut track_key_bytes = 0u64;
    let mut emitter_bytes = 0u64;
    let mut geoset_anim_bytes = 0u64;
    for model in &assets.models {
        if let Some(mdx) = model.model.as_ref() {
            mdx_models += 1;
            mdx_bones += mdx.bones.len() as u64;
            mdx_sequences += mdx.sequences.len() as u64;
            for node in mdx
                .bones
                .iter()
                .map(|bone| &bone.node)
                .chain(mdx.helpers.iter())
            {
                node_count += 1;
                track_key_bytes += node.translation.keys.len() as u64 * 40;
                track_key_bytes += node.rotation.keys.len() as u64 * 52;
                track_key_bytes += node.scaling.keys.len() as u64 * 40;
                track_key_bytes += node.name.capacity() as u64;
            }
            emitter_bytes += mdx.particle_emitters.len() as u64 * 512;
            emitter_bytes += mdx.particle_emitters2.len() as u64 * 1024;
            emitter_bytes += mdx.ribbon_emitters.len() as u64 * 512;
            geoset_anim_bytes += mdx.geoset_animations.len() as u64 * 256;
            for (_tag, data) in &mdx.raw_chunks {
                raw_chunk_count += 1;
                raw_chunk_bytes += data.len() as u64;
            }
            for g in &mdx.geosets {
                let v = g.vertices.len() as u64;
                geoset_bytes += v * 12 + g.normals.len() as u64 * 12;
                geoset_bytes += g.faces.len() as u64 * 2;
                geoset_bytes += g.uv_sets.iter().map(|s| s.len() as u64 * 8).sum::<u64>();
                geoset_bytes += g.skin.len() as u64 * 2;
                geoset_bytes += g.matrix_groups.len() as u64 * 4;
            }
        }
    }

    // ---- audio + UI --------------------------------------------------------
    let audio_bytes = assets
        .audio
        .as_ref()
        .map(|audio| audio.pcm_bytes() as u64)
        .unwrap_or(0);

    let asset_cpu = texture_bytes
        + terrain_tile_bytes
        + atlas_bytes
        + minimap_bytes
        + mesh_bytes
        + geoset_bytes
        + audio_bytes;
    println!("== mem_probe (asset layer) load={load_ms}ms ==");
    println!(
        "MODEL_SCOPE {} models={} roster={} placeholders={}",
        assets.model_scope.summary(),
        assets.loaded_model_count(),
        assets.model_roster,
        assets.placeholder_count()
    );
    println!(
        "textures={} bytes={:.1} MB | tiles={} {:.1} MB | atlas={:.1} MB | minimap={:.2} MB",
        assets.textures.len(),
        mb(texture_bytes),
        assets.terrain_tile_images.len(),
        mb(terrain_tile_bytes),
        mb(atlas_bytes),
        mb(minimap_bytes)
    );
    println!("top_textures={top:?}");
    // GPU-side budget: the renderer caps every surface at 256 and adds a mip
    // chain, so the uploaded size is `min(w,256) x min(h,256) * 4 * 4/3`.
    let mut histogram: Vec<((u32, u32), usize, u64)> = Vec::new();
    let mut gpu_bytes = 0u64;
    for image in &assets.textures {
        let capped = (image.width.min(256), image.height.min(256));
        let bytes = (capped.0 as u64) * (capped.1 as u64) * 4 * 4 / 3;
        gpu_bytes += bytes;
        match histogram.iter_mut().find(|(size, _, _)| *size == capped) {
            Some((_, count, total)) => {
                *count += 1;
                *total += bytes;
            }
            None => histogram.push((capped, 1, bytes)),
        }
    }
    histogram.sort_by_key(|(_, _, total)| std::cmp::Reverse(*total));
    let sizes: Vec<String> = histogram
        .iter()
        .take(8)
        .map(|((w, h), count, total)| format!("{w}x{h}x{count}={:.1}MB", mb(*total)))
        .collect();
    println!("gpu_texture_estimate={:.1} MB sizes={sizes:?}", mb(gpu_bytes));
    // Duplicate detection: identical decoded sheets (same frame art registered
    // under several aliases) upload twice and cost twice.
    let mut seen: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut duplicates = 0usize;
    let mut duplicate_bytes = 0u64;
    for image in &assets.textures {
        if image.rgba.is_empty() {
            continue;
        }
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in &image.rgba {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash ^= (image.width as u64) << 32 | image.height as u64;
        if seen.insert(hash, 1).is_some() {
            duplicates += 1;
            duplicate_bytes += image.rgba.len() as u64;
        }
    }
    println!(
        "duplicates={duplicates} duplicate_cpu_mb={:.1}",
        mb(duplicate_bytes)
    );
    println!(
        "meshes={} verts={} tris={} bytes={:.1} MB ({:.1} B/vertex, record {:.0} B) layers={}",
        mesh_count,
        verts,
        triangles,
        mb(mesh_bytes),
        bytes_per_vertex,
        vertex_record,
        mesh_layers
    );
    println!(
        "mdx_geosets={:.1} MB models={} nodes={} bones={} sequences={} track_keys={:.1} MB emitters={:.1} MB geoset_anim={:.1} MB raw_chunks={} {:.1} MB",
        mb(geoset_bytes),
        mdx_models,
        node_count,
        mdx_bones,
        mdx_sequences,
        mb(track_key_bytes),
        mb(emitter_bytes),
        mb(geoset_anim_bytes),
        raw_chunk_count,
        mb(raw_chunk_bytes)
    );
    println!("audio_pcm={:.1} MB", mb(audio_bytes));
    println!(
        "SUM asset_cpu={:.1} MB (textures {:.1} + tiles {:.1} + atlas {:.1} + mesh {:.1} + mdx {:.1} + audio {:.1})",
        mb(asset_cpu),
        mb(texture_bytes),
        mb(terrain_tile_bytes),
        mb(atlas_bytes),
        mb(mesh_bytes),
        mb(geoset_bytes),
        mb(audio_bytes)
    );
    println!(
        "units_known={} sound_defs={} ui_textures={}",
        assets.objects.as_ref().map(|o| o.units().count()).unwrap_or(0),
        assets.audio.as_ref().map(|a| a.sound_table().len()).unwrap_or(0),
        assets.ui.texture_count()
    );
    if let Some(audio) = assets.audio.as_ref() {
        let stats = audio.provider().stats();
        println!(
            "asset_db raw={:.1} MB tex={:.1} MB model={:.1} MB sound={:.1} MB entries={} sum={:.1} MB",
            mb(stats.bytes_cached as u64),
            mb(stats.texture_bytes as u64),
            mb(stats.model_bytes as u64),
            mb(stats.sound_bytes as u64),
            stats.cache_entries,
            mb((stats.bytes_cached + stats.texture_bytes + stats.model_bytes + stats.sound_bytes) as u64)
        );
        let (entries, bytes, evictions, budget) = audio.ambient_cache_stats();
        println!(
            "ambient_pcm entries={entries} bytes={:.1} MB evictions={evictions} budget={:.0} MB",
            mb(bytes as u64),
            mb(budget as u64)
        );
    }
    for line in &assets.debug {
        if line.contains("mem_stage") {
            println!("debug: {line}");
        }
    }

    for stage in &stages {
        println!("STAGE rss_{}={:.1} MB", stage.name, mb(stage.rss * 1024));
    }
    println!(
        "RSS after assets={:.1} MB (delta {:.1} MB) VmHWM={:.1} MB",
        mb(rss_kb() * 1024),
        mb(rss_kb().saturating_sub(stages[0].rss) * 1024),
        mb(hwm_kb() * 1024)
    );

    // ---- step 1: free the CPU copies the renderer no longer needs ----------
    // The app calls this right after `Renderer::new` has uploaded the textures.
    let before_release = rss_kb();
    let released = assets.release_cpu_textures();
    let after_release = rss_kb();
    stages.push(Stage {
        name: "released",
        rss: after_release,
    });
    println!(
        "release textures_cpu={:.1} MB surfaces={} asset_cache={:.1} MB -> rss={:.1} MB (delta {:.1} MB) textures_left={:.1} MB",
        mb(released.bytes as u64),
        released.surfaces,
        mb(released.cache_bytes as u64),
        mb(after_release * 1024),
        mb(before_release.saturating_sub(after_release) * 1024),
        mb(assets.cpu_texture_bytes() as u64)
    );
    println!(
        "after_release mesh_cpu={:.1} MB vertex_record={:.0} B bytes_per_vertex={:.1} (incl. indices) cpu_textures={:.1} MB",
        mb(assets.mesh_bytes() as u64),
        assets.vertex_record_bytes(),
        assets.bytes_per_vertex(),
        mb(assets.cpu_texture_bytes() as u64)
    );

    if budget {
        // Step 4: ten minutes of ambience playback in a headless loop. One
        // rendered theme is ~15.9 MB; the LRU must keep the peak under 64 MB
        // while the loop walks every tileset (region changes re-render).
        let mut peak_pcm = 0usize;
        let mut themes = 0usize;
        let mut rendered_blocks = 0u64;
        if let Some(audio) = assets.audio.as_mut() {
            let mut block = vec![0.0f32; 2048];
            let block_seconds = (block.len() / 2) as f32 / 44_100.0;
            let tilesets = ['A', 'B', 'K', 'L', 'N', 'Q', 'V', 'X'];
            let mut played = 0.0f32;
            let mut index = 0usize;
            while played < 600.0 {
                let tileset = tilesets[index % tilesets.len()];
                let night = (index / tilesets.len()) % 2 == 1;
                if audio.ambient_for_tileset(tileset, night).is_some() {
                    themes += 1;
                }
                // Render the equivalent of ten seconds of that region.
                let blocks = (10.0 / block_seconds).ceil() as usize;
                for _ in 0..blocks {
                    audio.render(&mut block);
                    rendered_blocks += 1;
                }
                played += blocks as f32 * block_seconds;
                peak_pcm = peak_pcm.max(audio.pcm_bytes());
                index += 1;
            }
        }
        println!(
            "audio_walk themes={themes} blocks={rendered_blocks} peak_pcm={:.1} MB budget=64 MB",
            mb(peak_pcm as u64)
        );
        if peak_pcm > 64 * 1024 * 1024 {
            println!(
                "BUDGET FAIL audio_pcm_peak={:.1} MB > 64 MB (step 4)",
                mb(peak_pcm as u64)
            );
            std::process::exit(1);
        }
    }

    if budget {
        // The budget describes the steady state the device measures after the
        // renderer has uploaded: CPU textures and mesh vertices are gone, the
        // vertex format is still the one the upload used.
        let failures = check_budget(
            vertex_record,
            cpu_bytes_per_vertex,
            assets.cpu_texture_bytes() as u64,
            assets.mesh_bytes() as u64,
            audio_bytes,
        );
        if failures.is_empty() {
            println!("BUDGET ok");
        } else {
            for line in &failures {
                println!("BUDGET FAIL {line}");
            }
            std::process::exit(1);
        }
    }
}

/// The budgets from `analysis/h2/MEMORY_BUDGET.md` steps 1–4.
/// Step 2 is checked two ways: the vertex *record* (the format the budget
/// names: i16/i8/u16/four influences) must be <= 21 B (20 B of vertex data plus
/// the 4 B second UV set on the 0.3 % of vertices that author one), and the
/// whole mesh CPU (record + `u16` index buffer + material metadata) must stay
/// <= 30 B/vertex (ledger 451 added the fourth 8-bit influence pair: +4 B).
fn check_budget(
    vertex_record: f64,
    bytes_per_vertex: f64,
    texture_bytes: u64,
    mesh_bytes: u64,
    audio_bytes: u64,
) -> Vec<String> {
    let mut failures = Vec::new();
    if texture_bytes > 0 {
        failures.push(format!(
            "textures_cpu={:.1} MB > 0 MB (step 1: free after upload)",
            mb(texture_bytes)
        ));
    }
    if vertex_record > 21.0 {
        failures.push(format!(
            "vertex_record={vertex_record:.1} B > 21 (step 2: quantised vertex format)"
        ));
    }
    if bytes_per_vertex > 30.0 {
        failures.push(format!(
            "bytes_per_vertex={bytes_per_vertex:.1} > 30 (step 2: mesh cpu with indices)"
        ));
    }
    if mesh_bytes > 150 * 1024 * 1024 {
        failures.push(format!(
            "mesh_cpu={:.1} MB > 150 MB (step 2)",
            mb(mesh_bytes)
        ));
    }
    if audio_bytes > 64 * 1024 * 1024 {
        failures.push(format!(
            "audio_pcm={:.1} MB > 64 MB (step 4: LRU cap)",
            mb(audio_bytes)
        ));
    }
    failures
}

/// `--frame`: render the real-map frame the visual gate analyses and report the
/// band statistics plus the dominant colours.
///
/// This is the tool the memory work uses to answer "did the release/quantise
/// change what is drawn": one frame, the same band `tests/visual.rs` measures,
/// and the top colours so a HUD panel covering the world is obvious.
fn frame_report() {
    let repo = Path::new("/data/data/com.termux/files/home/src/wc3rust");
    let mpq = [repo.join("analysis/war3_iso/War3.mpq"), Path::new("/data/data/org.wc3rust.p0/files/War3.mpq").to_path_buf()]
        .into_iter()
        .find(|path| path.is_file());
    let map = [repo.join("analysis/war3_map/Maps/Campaign/Human04.w3m"), repo.join("analysis/war3_map/Maps/Melee.w3x")]
        .into_iter()
        .find(|path| path.is_file());
    let (Some(mpq), Some(map)) = (mpq, map) else {
        println!("skip: install/map missing");
        return;
    };
    let assets = Assets::load(Some(&mpq), Some(&map));
    let mut game = wc3_game::Game::new(assets, 0x5EED);
    game.resize(960.0, 540.0);
    for _ in 0..30 {
        game.update(1.0 / 30.0);
    }
    let mut scene = game.scene();
    let (width, height) = (960u32, 540u32);
    // First pass with the HUD, then with an empty UI batch: a full-screen HUD
    // panel covering the world shows up as "green=0 with the HUD, green>0.18
    // without it", which is a HUD regression, not a terrain one.
    let full_ui_textured = scene.ui.textured.len();
    let full_ui_white = scene.ui.white.len();
    let no_ui = wc3_game::render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &{
            let mut copy = wc3_game::Scene {
                view_proj: scene.view_proj,
                eye: scene.eye,
                viewport: scene.viewport,
                units: Vec::new(),
                rings: Vec::new(),
                health: Vec::new(),
                ui: wc3_game::hud::UiBatch::default(),
                overlay: None,
            };
            copy.view_proj = scene.view_proj;
            copy
        },
        width,
        height,
    );
    let rendered = wc3_game::render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        width,
        height,
    );
    let Ok(rgba) = rendered else {
        println!("skip: renderer unavailable");
        return;
    };
    // Same band as tests/visual.rs.
    let (top, bottom) = ((height as f64 * 0.14) as u32, (height as f64 * 0.72) as u32);
    let (left, right) = ((width as f64 * 0.08) as u32, (width as f64 * 0.92) as u32);
    let analyse = |rgba: &[u8]| {
        let mut green = 0u64;
        let mut black = 0u64;
        let mut white = 0u64;
        let mut blue = 0u64;
        let mut total = 0u64;
        let mut buckets: Vec<((u8, u8, u8), u64)> = Vec::new();
        for y in top..bottom {
            for x in left..right {
                let index = ((y * width + x) * 4) as usize;
                let (r, g, b) = (rgba[index], rgba[index + 1], rgba[index + 2]);
                total += 1;
                if g as i32 > r as i32 + 8 && g as i32 > b as i32 + 4 && g > 40 {
                    green += 1;
                }
                if (r as u32 + g as u32 + b as u32) < 36 {
                    black += 1;
                }
                if r > 232 && g > 232 && b > 232 {
                    white += 1;
                }
                if b as i32 > r as i32 + 18 && b as i32 > g as i32 + 6 && b > 40 {
                    blue += 1;
                }
                let key = (r / 24, g / 24, b / 24);
                match buckets.iter_mut().find(|(slot, _)| *slot == key) {
                    Some((_, count)) => *count += 1,
                    None => buckets.push((key, 1)),
                }
            }
        }
        let total = total.max(1) as f64;
        buckets.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let top_colours: Vec<String> = buckets
            .iter()
            .take(5)
            .map(|((r, g, b), count)| {
                format!("({},{},{})x{}", r * 24 + 12, g * 24 + 12, b * 24 + 12, count)
            })
            .collect();
        (
            green as f64 / total,
            black as f64 / total,
            white as f64 / total,
            blue as f64 / total,
            buckets.len(),
            top_colours,
        )
    };
    let (green, black, white, blue, buckets, top_colours) = analyse(&rgba);
    println!(
        "FRAME phase={:?} screen={:?} loading={:.2} green={green:.3} black={black:.3} white={white:.3} blue={blue:.3} buckets={buckets} units={} ui_textured={} ui_white={}",
        game.phase,
        game.screen,
        game.loading_time,
        scene.units.len(),
        full_ui_textured,
        full_ui_white
    );
    println!("FRAME top_colours={top_colours:?}");
    // A full-screen panel is the failure mode this band analysis reports: find
    // which UI quad covers the viewport so the report names the draw site.
    let screen_area = (width * height) as f32;
    for (index, quad) in scene.ui.white.chunks_exact(6).enumerate() {
        let min = quad.iter().fold((f32::MAX, f32::MAX), |acc, vertex| {
            (acc.0.min(vertex.pos[0]), acc.1.min(vertex.pos[1]))
        });
        let max = quad.iter().fold((f32::MIN, f32::MIN), |acc, vertex| {
            (acc.0.max(vertex.pos[0]), acc.1.max(vertex.pos[1]))
        });
        let area = ((max.0 - min.0).max(0.0)) * ((max.1 - min.1).max(0.0));
        if area > screen_area * 0.5 {
            println!(
                "FRAME ui_quad#{index} area={:.2} of screen rect=({:.0},{:.0})-({:.0},{:.0}) colour={:?}",
                area / screen_area,
                min.0,
                min.1,
                max.0,
                max.1,
                quad[0].color
            );
        }
    }
    if let Ok(no_ui_rgba) = no_ui {
        let (green, black, white, blue, buckets, top_colours) = analyse(&no_ui_rgba);
        println!(
            "FRAME_NOUI green={green:.3} black={black:.3} white={white:.3} blue={blue:.3} buckets={buckets} units={}",
            scene.units.len()
        );
        println!("FRAME_NOUI top_colours={top_colours:?}");
    }
    let path = "/data/data/com.termux/files/usr/tmp/opencode/visual-frame.ppm";
    wc3_game::render::write_ppm(path, width, height, &rgba);
    println!("FRAME ppm={path}");
}

/// `--scope`: print the per-map model set the loader resolved, prove every
/// placed id has a model, and exercise the missing-model path once.
///
/// This is the ledger-435 evidence tool: the same `Assets::load` the app runs,
/// with the reachable set and the placeholder line printed instead of inferred.
fn scope_report() {
    let install = std::env::var_os("H2_MEM_INSTALL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new("/data/data/org.wc3rust.p0/files").to_path_buf());
    let map = std::env::var_os("H2_MEM_MAP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| install.join("Melee.w3x"));
    if !install.join("War3.mpq").is_file() || !map.is_file() {
        println!("skip: install/map missing");
        return;
    }
    let started = std::time::Instant::now();
    let assets = Assets::load(Some(&install.join("War3.mpq")), Some(&map));
    let load_ms = started.elapsed().as_millis();
    println!(
        "SCOPE map={:?} load={load_ms}ms {}",
        map,
        assets.model_scope.summary()
    );
    let mdx_bytes: usize = assets
        .models
        .iter()
        .filter_map(|model| model.model.as_ref())
        .map(|mdx| {
            let mut bytes = mdx.sequences.len() * std::mem::size_of::<wc3_formats::mdx::Sequence>();
            for bone in &mdx.bones {
                bytes += bone.node.translation.keys.len() * 40;
                bytes += bone.node.rotation.keys.len() * 52;
                bytes += bone.node.scaling.keys.len() * 40;
            }
            bytes
        })
        .sum();
    println!(
        "SCOPE models={} roster={} placeholders={} mdx_parsed~={:.1} MB",
        assets.loaded_model_count(),
        assets.model_roster,
        assets.placeholder_count(),
        mb(mdx_bytes as u64)
    );
    let ids: Vec<&str> = assets.model_scope.ids.iter().map(String::as_str).collect();
    println!("SCOPE ids={ids:?}");
    let mut placed_without_model = Vec::new();
    for id in &assets.model_scope.ids {
        if wc3_sim::data::def_of(id).is_none() {
            continue;
        }
        if !assets.model_by_def.contains_key(id) {
            placed_without_model.push(id.clone());
        }
    }
    println!("SCOPE placed_without_model={placed_without_model:?}");
    // Missing-model path: the first def outside the set resolves to a box and
    // logs exactly one line, however often it is asked.
    let probe = assets
        .model_by_def
        .iter()
        .filter(|(id, index)| {
            assets.models[**index].fallback && wc3_sim::data::def_of(id).is_some()
        })
        .map(|(id, _)| id.clone())
        .min();
    if let Some(def) = probe {
        let first = assets.model_for_def(&def);
        let height = first.height;
        let meshes = first.meshes.len();
        let placeholder = first.fallback;
        let again = assets.model_for_def(&def).fallback;
        println!(
            "SCOPE placeholder def={def} placeholder={placeholder} height={height:.1} meshes={meshes} second_call_placeholder={again}"
        );
        let log = assets.model_skip_log();
        println!("SCOPE skip_log lines={} {:?}", log.len(), log);
    }
    println!("SCOPE rss={:.1} MB", wc3_game::mem::rss_mb());
    // The load is also the place to prove the roster fallback still works.
    let full = Assets::load(None, Some(&map));
    println!(
        "SCOPE no-mpq models={} roster={} placeholders={} mode={}",
        full.loaded_model_count(),
        full.model_roster,
        full.placeholder_count(),
        full.model_scope.mode.label()
    );
}

/// `--breakdown`: after the upload release, measure what the remaining resident
/// bytes actually belong to by dropping one layer at a time.
///
/// The game itself is not started, so the numbers are the asset layer only; a
/// layer's cost is the RSS delta when it is dropped (`objects`, the MDX parsed
/// models, the FDF/UI theme, the audio wiring).
fn breakdown() {
    let install = std::env::var_os("H2_MEM_INSTALL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new("/data/data/org.wc3rust.p0/files").to_path_buf());
    let map = std::env::var_os("H2_MEM_MAP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| install.join("Melee.w3x"));
    if !install.join("War3.mpq").is_file() || !map.is_file() {
        println!("skip: install/map missing");
        return;
    }
    let mut assets = Assets::load(Some(&install.join("War3.mpq")), Some(&map));
    let released = assets.release_cpu_textures();
    println!(
        "BREAKDOWN map={:?} scope={} models={} roster={} placeholders={}",
        map,
        assets.model_scope.summary(),
        assets.loaded_model_count(),
        assets.model_roster,
        assets.placeholder_count()
    );
    println!(
        "BREAKDOWN after_release rss={:.1} MB (freed textures {:.1} + cache {:.1} + mesh {:.1})",
        wc3_game::mem::rss_mb(),
        mb(released.bytes as u64),
        mb(released.cache_bytes as u64),
        mb(released.mesh_bytes as u64)
    );
    let step = |label: &str, before: f64| {
        let after = wc3_game::mem::rss_mb();
        println!(
            "BREAKDOWN drop {label}: -{:.1} MB (rss {:.1} MB)",
            before - after,
            after
        );
    };
    // Objects (unit/ability/item rows with their localised strings).
    let mut before = wc3_game::mem::rss_mb();
    if assets.objects.take().is_some() {
        step("objects", before);
    }
    // Parsed MDX models (animator input: bones, helpers, tracks, sequences).
    before = wc3_game::mem::rss_mb();
    for model in &mut assets.models {
        model.model = None;
    }
    step("mdx_models", before);
    // FDF UI theme (documents, templates, strings, skin table).
    before = wc3_game::mem::rss_mb();
    assets.ui = wc3_game::ui::UiTheme::default();
    step("ui_theme", before);
    // Audio wiring (sound info tables, unit name map, provider handle).
    before = wc3_game::mem::rss_mb();
    assets.audio = None;
    step("audio", before);
    println!(
        "BREAKDOWN final rss={:.1} MB models={} textures={}",
        wc3_game::mem::rss_mb(),
        assets.models.len(),
        assets.textures.len()
    );
}
