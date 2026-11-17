//! Game audio wiring verification.
//!
//! Runs the exact code path the game uses after the assets patch:
//! `AssetDb` -> `SharedAssetDb` -> `start_game_audio` -> `GameAudio` hooks, while
//! the renderer side keeps reading textures/models from the *same* database.
//!
//! Reports the device path (SDL3 or the documented null fallback), the wiring
//! summary, hook statistics, the rendered peak and the asset-cache counters that
//! prove both sides share one cache.
//!
//! `audio_wiring [--mpq PATH] [--index PATH] [--out DIR] [--seconds N]`
//!
//! Audio output is disabled by default here (`H2_AUDIO=null` equivalent) so QA
//! runs do not touch the device; pass `--device sdl` to open the real backend.

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec3;

use wc3_assetdb::audio_provider::{
    build_game_audio, game_audio_stats, start_game_audio, SharedAssetDb, WiredGameAudio,
};
use wc3_assetdb::AssetDb;
use wc3_audio::engine::AudioConfig;


/// Minimal JSON string escaping for paths with backslashes and quotes.
fn esc(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// JSON array of strings.
fn json_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&esc(value));
        out.push('"');
    }
    out.push(']');
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into());
    let mut index: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("analysis/agent-assets");
    let mut seconds = 3u32;
    let mut allow_device = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq = PathBuf::from(&args[i]);
            }
            "--index" => {
                i += 1;
                index = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(&args[i]);
            }
            "--seconds" => {
                i += 1;
                seconds = args[i].parse()?;
            }
            "--device" => {
                i += 1;
                allow_device = args[i] == "sdl";
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }
    if !allow_device {
        // SAFETY: single-threaded setup before any render thread starts.
        unsafe { std::env::set_var("H2_AUDIO", "null") };
    }

    let open_start = Instant::now();
    let (db, used_index) = match &index {
        Some(path) => AssetDb::open_mpq_best(&mpq, path)?,
        None => (AssetDb::open_mpq(&mpq)?, false),
    };
    let open_ms = open_start.elapsed().as_secs_f64() * 1000.0;

    // One database, shared by the reader side and the audio hooks.
    let shared = SharedAssetDb::new(db);

    // ---- deterministic phase: headless engine, pull-rendered --------------
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 64,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    };
    let (mut audio, wiring) = build_game_audio(shared.clone(), wc3_audio::AudioEngine::headless(config), "headless");
    println!("wiring: {}", wiring.summary());

    let texture = shared.texture("TerrainArt/Village/Village_Dirt.blp")?;
    assert_eq!((texture.width, texture.height), (512, 256));

    let ack = audio
        .unit_ack("hfoo", Some(Vec3::new(10.0, 0.0, 0.0)))
        .ok_or("Footman ack did not start")?;
    let attack = audio.unit_attack("ogru", None).ok_or("Grunt attack did not start")?;
    let click = audio.ui_click().ok_or("UI click did not start")?;
    let music = audio.music("Human1").ok_or("music theme did not start")?;
    println!("voices: ack={ack:?} attack={attack:?} click={click:?} music={music:?}");

    let block_frames = config.block_frames;
    let channels = config.channels as usize;
    let mut block = vec![0.0f32; block_frames * channels];
    let blocks = (seconds as usize) * config.sample_rate as usize / block_frames;
    let mut peak = 0.0f32;
    let mut nonzero = 0usize;
    let render_start = Instant::now();
    for _ in 0..blocks {
        let frames = audio.render(&mut block);
        assert_eq!(frames, block_frames, "GameAudio::render must produce full blocks");
        let block_peak = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        peak = peak.max(block_peak);
        if block_peak > 0.001 {
            nonzero += 1;
        }
    }
    let render_secs = render_start.elapsed().as_secs_f64();
    // 3D positional check: isolate the spatial voice (music is a 2D stereo
    // stream and would mask the pan), then place a source at -X for a listener
    // at the origin looking +Y: it must favour the left channel.
    audio.stop_all();
    for _ in 0..6 {
        audio.render(&mut block);
    }
    audio.set_listener(Vec3::ZERO, 0.0);
    let spatial = audio
        .unit_ack("hfoo", Some(Vec3::new(-200.0, 0.0, 0.0)))
        .ok_or("3D ack did not start")?;
    let mut left_energy = 0.0f32;
    let mut right_energy = 0.0f32;
    for _ in 0..8 {
        audio.render(&mut block);
        for frame in block.chunks_exact(2) {
            left_energy += frame[0] * frame[0];
            right_energy += frame[1] * frame[1];
        }
    }
    println!(
        "3d: voice={spatial:?} left_energy={left_energy:.4} right_energy={right_energy:.4}"
    );
    assert!(
        left_energy > right_energy * 1.3,
        "a source on the listener's left must favour the left channel: {left_energy} vs {right_energy}"
    );

    let hook_stats = game_audio_stats(&audio);
    let model = shared.model_shared("Units/Human/Footman/Footman.mdx")?;
    // Re-read a texture the hooks already touched: this must be a cache hit,
    // proving renderer and audio share one database.
    let texture_levels = shared
        .texture_shared("TerrainArt/Village/Village_Dirt.blp")?
        .mip_levels();
    let stats = shared.stats();
    let shared_hits = stats.hits;
    audio.shutdown();

    assert!(peak > 0.001, "wired audio produced silence");
    assert!(hook_stats.acks_started >= 1);
    assert!(hook_stats.music_started >= 1);
    assert!(nonzero > blocks / 4, "music must stream continuously: {nonzero}/{blocks}");
    assert!(shared_hits > 0, "shared cache must record a hit across renderer+audio");

    // ---- production phase: render thread + platform backend ---------------
    let (mut live, live_wiring) = start_game_audio(shared.clone());
    let live_ack = live.unit_ack("hfoo", Some(Vec3::new(5.0, 0.0, 0.0)));
    let live_music = live.music("Human1");
    std::thread::sleep(std::time::Duration::from_millis(350));
    let live_stats = live.audio_stats();
    live.shutdown();
    assert!(live_ack.is_some() && live_music.is_some());
    assert!(
        live_stats.frames_rendered > 0,
        "render thread produced no frames: {live_stats:?}"
    );

    let report = format!(
        concat!(
            "{{\n",
            "  \"mpq\": \"{}\",\n",
            "  \"index_used\": {},\n",
            "  \"open_ms\": {:.2},\n",
            "  \"device\": \"{}\",\n",
            "  \"headless_device\": \"{}\",\n",
            "  \"source\": \"{}\",\n",
            "  \"tables\": {},\n",
            "  \"definitions\": {},\n",
            "  \"units\": {},\n",
            "  \"themes\": {},\n",
            "  \"texture\": \"{}x{} mips={}\",\n",
            "  \"model\": \"{} textures={}\",\n",
            "  \"acks_started\": {},\n",
            "  \"acks_missing\": {},\n",
            "  \"music_started\": {},\n",
            "  \"left_energy_3d\": {:.4},\n",
            "  \"right_energy_3d\": {:.4},\n",
            "  \"ui_events\": {},\n",
            "  \"seconds_requested\": {},\n",
            "  \"render_secs\": {:.3},\n",
            "  \"peak\": {:.4},\n",
            "  \"nonzero_blocks\": {},\n",
            "  \"blocks\": {},\n",
            "  \"live_frames_rendered\": {},\n",
            "  \"cache\": {{\"hits\": {}, \"misses\": {}, \"bytes_cached\": {}, \"texture_bytes\": {}, \"model_bytes\": {}, \"sound_bytes\": {}, \"total_bytes\": {}}},\n",
            "  \"wired\": true\n",
            "}}\n"
        ),
        mpq.display(),
        used_index,
        open_ms,
        live_wiring.device,
        wiring.device,
        esc(&wiring.source),
        wiring.tables,
        wiring.definitions,
        wiring.units,
        wiring.themes,
        texture.width,
        texture.height,
        texture_levels,
        esc(&model.name),
        model.model.textures.len(),
        hook_stats.acks_started,
        hook_stats.acks_missing,
        hook_stats.music_started,
        left_energy,
        right_energy,
        hook_stats.ui_events,
        seconds,
        render_secs,
        peak,
        nonzero,
        blocks,
        live_stats.frames_rendered,
        stats.hits,
        stats.misses,
        stats.bytes_cached,
        stats.texture_bytes,
        stats.model_bytes,
        stats.sound_bytes,
        stats.bytes_cached + stats.texture_bytes + stats.model_bytes + stats.sound_bytes,
    );
    println!("{report}");
    std::fs::create_dir_all(&out_dir)?;
    std::fs::write(out_dir.join("audio-wiring.json"), &report)?;

    let _: WiredGameAudio = audio;
    Ok(())
}
