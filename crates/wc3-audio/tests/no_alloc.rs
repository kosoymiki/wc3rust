//! Proves the render path does not allocate per block/frame.
//!
//! A counting global allocator instruments the current thread only (via a
//! const-initialised `thread_local!` flag), so the parallel test harness cannot
//! pollute the count. After a warm-up (commands drained, buffers sized), every
//! `render` call must perform **zero** heap allocations and zero frees.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use glam::Vec3;
use wc3_audio::engine::{AudioConfig, AudioEngine, PlayParams};
use wc3_audio::Sound;

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);

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
        if MEASURING.with(|flag| flag.get()) {
            FREES.fetch_add(1, Ordering::Relaxed);
        }
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

fn tone(frames: usize, rate: u32, channels: u16) -> Arc<Sound> {
    let mut samples = Vec::with_capacity(frames * channels as usize);
    for frame in 0..frames {
        let value = (frame as f32 * 0.01).sin() * 0.5;
        for _ in 0..channels {
            samples.push(value);
        }
    }
    Arc::new(Sound {
        sample_rate: rate,
        channels,
        samples,
    })
}

#[test]
fn render_allocates_nothing_after_warmup() {
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 32,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.9,
    };
    let mut engine = AudioEngine::headless(config);
    let sound = tone(48_000, 44_100, 2);
    // 16 voices with a mix of 2D/3D/looping/pan so every path is exercised.
    for index in 0..16 {
        let params = if index % 3 == 0 {
            PlayParams {
                position: Some(Vec3::new(index as f32 * 10.0 - 80.0, 20.0, 5.0)),
                reference_distance: 100.0,
                ..Default::default()
            }
        } else {
            PlayParams {
                pan: (index as f32 / 8.0) - 1.0,
                looping: index % 2 == 0,
                gain: 0.05,
                ..Default::default()
            }
        };
        engine.play(sound.clone(), params);
    }
    let mut block = vec![0.0f32; config.block_samples()];
    // Warm-up: drain commands, grow any lazily sized buffers, settle envelopes.
    for _ in 0..8 {
        engine.render(&mut block);
    }

    MEASURING.with(|flag| flag.set(true));
    for _ in 0..64 {
        engine.render(&mut block);
    }
    MEASURING.with(|flag| flag.set(false));

    let allocs = ALLOCS.load(Ordering::Relaxed);
    let frees = FREES.load(Ordering::Relaxed);
    let stats = engine.stats();
    println!(
        "allocations={allocs} frees={frees} frames={} voices={}",
        stats.frames_rendered, stats.active_voices
    );
    assert_eq!(allocs, 0, "render path allocated {allocs} times");
    assert_eq!(frees, 0, "render path freed {frees} times");
    assert!(stats.frames_rendered > 0);
    assert!(stats.active_voices > 0, "voices must still be running");
}
