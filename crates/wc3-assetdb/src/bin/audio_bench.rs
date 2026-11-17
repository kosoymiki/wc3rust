//! Audio/asset performance and memory evidence.
//!
//! Measures, with real archive data:
//! * MPQ+index open time and sound-table parse time
//! * WAV decode throughput over a representative slice of the corpus
//! * bounded sound-cache behaviour (budget respected, evictions happen)
//! * 60 s of 32-voice mixing: wall time, realtime factor, allocations during
//!   render (must be zero after warm-up), RSS
//! * streaming MP3 music: RSS while playing a multi-minute track
//!
//! Usage: `audio_bench [--mpq PATH] [--index PATH] [--out DIR] [--voices N]
//!                     [--seconds S] [--wavs N]`

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use wc3_assetdb::audio_provider::load_sound_tables;
use wc3_assetdb::{AssetDb, CacheConfig};
use wc3_audio::engine::{AudioConfig, AudioEngine, PlayParams};
use wc3_audio::{probe_mp3, Sound};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static MEASURING: Cell<bool> = const { Cell::new(false) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MEASURING.with(|flag| flag.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if MEASURING.with(|flag| flag.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if MEASURING.with(|flag| flag.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn rss_kb() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let mut rss = 0u64;
    let mut peak = 0u64;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            rss = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("VmHWM:") {
            peak = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
    }
    (rss, peak)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into());
    let mut index: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("analysis/agent-assets");
    let mut voices = 32usize;
    let mut seconds = 60u64;
    let mut wav_limit = 300usize;
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
            "--voices" => {
                i += 1;
                voices = args[i].parse()?;
            }
            "--seconds" => {
                i += 1;
                seconds = args[i].parse()?;
            }
            "--wavs" => {
                i += 1;
                wav_limit = args[i].parse()?;
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }
    std::fs::create_dir_all(&out_dir)?;
    let (rss_start, _) = rss_kb();

    // ---- open ---------------------------------------------------------------------
    let t = Instant::now();
    let (mut db, used_index) = match &index {
        Some(path) => AssetDb::open_mpq_best(&mpq, path)?,
        None => (AssetDb::open_mpq(&mpq)?, false),
    };
    let open_ms = t.elapsed().as_secs_f64() * 1000.0;

    // ---- sound tables -------------------------------------------------------------
    let t = Instant::now();
    let (sounds, units, tables) = load_sound_tables(&mut db);
    let table_ms = t.elapsed().as_secs_f64() * 1000.0;

    // ---- WAV decode throughput ----------------------------------------------------
    let files = db.files();
    let wav_names: Vec<String> = files
        .iter()
        .filter(|entry| entry.name.to_ascii_lowercase().ends_with(".wav"))
        .map(|entry| entry.name.clone())
        .take(wav_limit)
        .collect();
    let t = Instant::now();
    let mut decoded_frames = 0u64;
    let mut decoded_bytes = 0u64;
    let mut source_bytes = 0u64;
    let mut codecs: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_sound: Option<Arc<Sound>> = None;
    for (n, name) in wav_names.iter().enumerate() {
        let bytes = db.read_uncached(name)?;
        source_bytes += bytes.len() as u64;
        if let Ok(info) = wc3_audio::probe_wav(&bytes) {
            *codecs.entry(info.codec.name().to_string()).or_default() += 1;
        }
        // Decode without the cache: this phase measures raw decode throughput,
        // not cache behaviour (the bounded-cache phase does that).
        let sound = wc3_audio::decode_wav(&bytes)?;
        decoded_frames += sound.frames() as u64;
        decoded_bytes += sound.memory_bytes() as u64;
        if n == 0 {
            first_sound = Some(Arc::new(sound));
        }
    }
    let wav_ms = t.elapsed().as_secs_f64() * 1000.0;

    // ---- bounded cache ------------------------------------------------------------
    let mut bounded = AssetDb::open_mpq(&mpq)?.with_config(CacheConfig {
        byte_budget: 4 * 1024 * 1024,
        texture_budget: 4 * 1024 * 1024,
        model_budget: 4 * 1024 * 1024,
        sound_budget: 2 * 1024 * 1024,
        max_sound_seconds: 5.0,
        max_entries: 64,
        negative_cache: true,
    });
    let t = Instant::now();
    let mut loaded = 0usize;
    for name in wav_names.iter().take(150) {
        if bounded.sound_shared(name).is_ok() {
            loaded += 1;
        }
    }
    let bounded_ms = t.elapsed().as_secs_f64() * 1000.0;
    let stats = bounded.stats();
    let total_bytes = bounded.total_bytes();

    // ---- 32-voice mixing ----------------------------------------------------------
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: voices.max(8),
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    };
    let mut engine = AudioEngine::headless(config);
    let base = first_sound.expect("at least one decoded sound");
    for slot in 0..voices {
        let params = if slot % 2 == 0 {
            PlayParams {
                position: Some(Vec3::new(((slot as f32) - 16.0) * 40.0, 30.0, 10.0)),
                reference_distance: 400.0,
                looping: true,
                priority: 5,
                ..Default::default()
            }
        } else {
            PlayParams {
                pan: ((slot as f32) / voices as f32) * 2.0 - 1.0,
                looping: true,
                gain: 0.1,
                ..Default::default()
            }
        };
        engine.play(base.clone(), params);
    }
    let mut block = vec![0.0f32; config.block_samples()];
    let blocks = (seconds * config.sample_rate as u64 / config.block_frames as u64) as usize;
    // Warm-up (command drain + envelope).
    for _ in 0..8 {
        engine.render(&mut block);
    }
    let (rss_before_render, _) = rss_kb();
    MEASURING.with(|flag| flag.set(true));
    let mut mix_peak = 0.0f32;
    let t = Instant::now();
    for _ in 0..blocks {
        engine.render(&mut block);
        mix_peak = block.iter().fold(mix_peak, |p, s| p.max(s.abs()));
    }
    let render_secs = t.elapsed().as_secs_f64();
    MEASURING.with(|flag| flag.set(false));
    let render_allocs = ALLOCS.load(Ordering::Relaxed);
    let (rss_after_render, peak_render) = rss_kb();
    let audio_stats = engine.stats();

    // ---- streaming music memory ---------------------------------------------------
    let music_name = "Sound\\Music\\mp3Music\\Human1.mp3";
    let mut music_result = String::from("unavailable");
    if let Ok(bytes) = db.read_uncached(music_name) {
        let info = probe_mp3(&bytes);
        let rss_before_music = rss_kb().0;
        let mut music_engine = AudioEngine::headless(AudioConfig {
            sample_rate: 48_000,
            channels: 2,
            block_frames: 1024,
            max_voices: 4,
            channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.7,
        });
        let handle = music_engine.play_mp3(
            Arc::from(bytes.as_slice()),
            PlayParams {
                category: wc3_audio::Category::Music,
                looping: true,
                gain: 0.7,
                ..Default::default()
            },
        )?;
        let _ = handle;
        let mut music_block = vec![0.0f32; 1024 * 2];
        for _ in 0..10 {
            music_engine.render(&mut music_block);
        }
        MEASURING.with(|flag| flag.set(true));
        let t = Instant::now();
        for _ in 0..(((seconds / 2).max(10)) * 48_000 / 1024) {
            music_engine.render(&mut music_block);
        }
        let music_render_secs = t.elapsed().as_secs_f64();
        MEASURING.with(|flag| flag.set(false));
        let music_alloc_probe = ALLOCS.load(Ordering::Relaxed) - render_allocs;
        let rss_after_music = rss_kb().0;
        let music_blocks = ((seconds / 2).max(10) * 48_000 / 1024) as f64;
        let streamed_secs = music_blocks * 1024.0 / 48_000.0;
        music_result = format!(
            "{} Hz {} ch, {:.1} s streamed in {:.2} s ({:.1}x realtime), rss {} -> {} kB, allocs {}",
            info.as_ref().map(|i| i.sample_rate).unwrap_or(0),
            info.as_ref().map(|i| i.channels).unwrap_or(0),
            streamed_secs,
            music_render_secs,
            streamed_secs / music_render_secs.max(1e-6),
            rss_before_music,
            rss_after_music,
            music_alloc_probe
        );
    }

    let (rss_end, peak_end) = rss_kb();
    let report = format!(
        concat!(
            "{{\n",
            "  \"mpq\": \"{}\",\n",
            "  \"index_used\": {},\n",
            "  \"open_ms\": {:.2},\n",
            "  \"sound_tables\": {{\"definitions\": {}, \"units\": {}, \"tables\": {}, \"parse_ms\": {:.2}}},\n",
            "  \"wav\": {{\"files\": {}, \"source_bytes\": {}, \"decoded_frames\": {}, \"decoded_bytes\": {}, \"decode_ms\": {:.2}, \"frames_per_sec\": {:.0}, \"codecs\": {:?}}},\n",
            "  \"bounded_cache\": {{\"loaded\": {}, \"total_bytes\": {}, \"sound_bytes\": {}, \"raw_bytes\": {}, \"sound_evictions\": {}, \"raw_evictions\": {}, \"load_ms\": {:.2}}},\n",
            "  \"mix\": {{\"voices\": {}, \"blocks\": {}, \"seconds\": {}, \"render_secs\": {:.3}, \"realtime_factor\": {:.1}, \"allocs_during_render\": {}, \"frames\": {}, \"peak\": {:.3}, \"max_active\": {}, \"underruns\": {}}},\n",
            "  \"music_stream\": \"{}\",\n",
            "  \"rss_kb_start\": {}, \"rss_kb_before_render\": {}, \"rss_kb_after_render\": {}, \"rss_kb_end\": {}, \"peak_rss_kb\": {}\n",
            "}}\n"
        ),
        mpq.display(),
        used_index,
        open_ms,
        sounds.len(),
        units.len(),
        tables,
        table_ms,
        wav_names.len(),
        source_bytes,
        decoded_frames,
        decoded_bytes,
        wav_ms,
        decoded_frames as f64 / (wav_ms / 1000.0),
        codecs,
        loaded,
        total_bytes,
        bounded.sound_bytes(),
        bounded.cached_bytes(),
        stats.sound_evictions,
        stats.raw_evictions,
        bounded_ms,
        voices,
        blocks,
        seconds,
        render_secs,
        seconds as f64 / render_secs,
        render_allocs,
        audio_stats.frames_rendered,
        mix_peak,
        audio_stats.max_active_voices,
        audio_stats.backend_errors,
        music_result,
        rss_start,
        rss_before_render,
        rss_after_render,
        rss_end,
        peak_end.max(peak_render),
    );
    println!("{report}");
    std::fs::write(out_dir.join("audio-bench.json"), &report)?;
    let _ = rss_after_render;
    Ok(())
}
