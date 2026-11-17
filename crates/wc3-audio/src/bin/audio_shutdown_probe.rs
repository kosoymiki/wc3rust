//! Shutdown probe: opens the platform audio backend, plays one voice, drops the
//! engine and prints a marker after the drop. Used by `tests/shutdown.rs` to
//! prove the process exits (and how fast) with both the real SDL3 backend and
//! `H2_AUDIO=null`.
//!
//!   cargo run -p wc3-audio --bin audio_shutdown_probe
//!   H2_AUDIO=null cargo run -p wc3-audio --bin audio_shutdown_probe

use std::sync::Arc;
use std::time::{Duration, Instant};

use wc3_audio::engine::open_default_backend;
use wc3_audio::{AudioConfig, AudioEngine, PlayParams, Sound};

fn main() {
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 8,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.5,
    };
    let (backend, name) = open_default_backend(&config);
    println!("PROBE_BACKEND {name}");
    let mut engine = AudioEngine::start(config, backend);
    let sound = Arc::new(Sound {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0.1f32; 4_800],
    });
    engine.play(
        sound,
        PlayParams {
            looping: true,
            ..Default::default()
        },
    );
    std::thread::sleep(Duration::from_millis(250));
    println!("PROBE_DROP_BEGIN");
    let started = Instant::now();
    drop(engine);
    println!("PROBE_DROPPED ms={}", started.elapsed().as_millis());
    println!("PROBE_EXIT");
}
