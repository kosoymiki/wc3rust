//! Real-device probe for the playback queue: opens the SDL3 backend, plays a
//! looping tone for N seconds, and reports queue depth, rendered frames and
//! buffer underruns. This is the device verification for the crackle fix
//! (queue-depth pacing with a 150 ms target instead of one block).
//!
//! Usage: `audio_underrun_probe [--seconds N] [--json]`
//! Exit: 0 when the device backend ran and no dropout was observed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use wc3_audio::engine::{open_default_backend, AudioConfig, AudioEngine, PlayParams};
use wc3_audio::Sound;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seconds = 5.0f64;
    let mut json_only = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--seconds" => {
                i += 1;
                seconds = args.get(i).ok_or("--seconds needs a value")?.parse()?;
            }
            "--json" => json_only = true,
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }
    if !(0.5..=60.0).contains(&seconds) {
        return Err("--seconds must be within 0.5..=60".into());
    }

    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 64,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    };
    let (backend, device) = open_default_backend(&config);
    let target_ms = config.effective_buffer_ms();
    let mut engine = AudioEngine::start(config, backend);

    let rate = config.sample_rate;
    let len = rate as usize; // one second, looped
    let samples: Vec<f32> = (0..len)
        .map(|n| (n as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin() * 0.15)
        .collect();
    let sound = Arc::new(Sound {
        sample_rate: rate,
        channels: 1,
        samples,
    });
    let voice = engine.play(
        sound,
        PlayParams {
            looping: true,
            category: wc3_audio::Category::Sfx,
            ..Default::default()
        },
    );

    let mut min_buffered = u32::MAX;
    let mut max_buffered = 0u32;
    let mut samples_taken = 0u32;
    let start = Instant::now();
    let mut last_log = Instant::now();
    while start.elapsed() < Duration::from_secs_f64(seconds) {
        std::thread::sleep(Duration::from_millis(50));
        if let Some(ms) = engine.buffered_ms() {
            min_buffered = min_buffered.min(ms);
            max_buffered = max_buffered.max(ms);
            samples_taken += 1;
        }
        if !json_only && last_log.elapsed() >= Duration::from_millis(500) {
            last_log = Instant::now();
            let stats = engine.stats();
            eprintln!(
                "PROBE t={:.2}s buffered_ms={:?} frames={} underruns={} active={}",
                start.elapsed().as_secs_f64(),
                engine.buffered_ms(),
                stats.frames_rendered,
                stats.buffer_underruns,
                stats.active_voices
            );
        }
    }
    engine.stop(voice);
    std::thread::sleep(Duration::from_millis(50));
    let stats = engine.stats();
    let elapsed = start.elapsed().as_secs_f64();
    let expected_frames = (elapsed * config.sample_rate as f64) as u64;
    if min_buffered == u32::MAX {
        min_buffered = 0;
    }
    println!(
        "PROBE_JSON {{\"backend\":\"{device}\",\"seconds\":{elapsed:.3},\"target_ms\":{target_ms},\
         \"min_buffered_ms\":{min_buffered},\"max_buffered_ms\":{max_buffered},\
         \"samples\":{samples_taken},\"frames_rendered\":{},\"expected_frames\":{expected_frames},\
         \"underruns\":{},\"backend_errors\":{},\"blocks\":{}}}",
        stats.frames_rendered,
        stats.buffer_underruns,
        stats.backend_errors,
        stats.blocks_rendered
    );

    let frames_ok = stats.frames_rendered as f64 >= expected_frames as f64 * 0.9;
    let underruns_ok = stats.buffer_underruns == 0;
    let backend_ok = device == "sdl3";
    if !backend_ok || !frames_ok || !underruns_ok {
        return Err(format!(
            "probe failed: backend={device} (sdl3 required) frames={}/{} underruns={}",
            stats.frames_rendered, expected_frames, stats.buffer_underruns
        )
        .into());
    }
    Ok(())
}
