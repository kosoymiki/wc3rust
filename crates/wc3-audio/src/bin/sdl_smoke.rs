//! SDL3 playback smoke test: opens the real device, plays a tone, reports what
//! the backend did.
//!
//! `cargo run -p wc3-audio --bin sdl_smoke`
//!
//! On Termux this exercises `$PREFIX/lib/libSDL3.so`; inside the Android app the
//! same code finds the bundled `libSDL3.so` because SDL is already mapped into
//! the process.

use std::sync::Arc;
use std::time::{Duration, Instant};

use wc3_audio::engine::{AudioBackend, AudioConfig, AudioEngine, PlayParams};
use wc3_audio::engine::sdl3::Sdl3Backend;
use wc3_audio::Sound;

fn tone(frames: usize, rate: u32, freq: f32) -> Arc<Sound> {
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let value = (frame as f32 * freq * std::f32::consts::TAU / rate as f32).sin() * 0.25;
        samples.push(value);
        samples.push(value);
    }
    Arc::new(Sound {
        sample_rate: rate,
        channels: 2,
        samples,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("sdl3: {}", Sdl3Backend::describe()?);
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 8,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    };
    let backend = Sdl3Backend::open(&config)?;
    println!(
        "backend={} latency={}ms",
        backend.name(),
        backend.latency_ms()
    );
    let mut engine = AudioEngine::start(config, Box::new(backend));
    let base_tone = tone(24_000, 48_000, 440.0);
    for (n, freq) in [440.0f32, 660.0, 880.0].iter().enumerate() {
        let sound = if (*freq - 440.0).abs() < 0.1 {
            base_tone.clone()
        } else {
            tone(24_000, 48_000, *freq)
        };
        engine.play(
            sound,
            PlayParams {
                gain: 0.3,
                pan: (n as f32) - 1.0,
                ..Default::default()
            },
        );
    }
    let started = Instant::now();
    std::thread::sleep(Duration::from_millis(900));
    let stats = engine.stats();
    engine.shutdown();
    println!(
        "played {:.2}s wall, frames={} blocks={} started={} finished={} errors={}",
        started.elapsed().as_secs_f32(),
        stats.frames_rendered,
        stats.blocks_rendered,
        stats.voices_started,
        stats.voices_finished,
        stats.backend_errors
    );
    if stats.frames_rendered == 0 || stats.backend_errors > 0 {
        return Err("SDL3 backend did not render audio".into());
    }
    Ok(())
}
