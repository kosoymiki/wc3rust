//! WC3-I-011 probe: pitch bounds, MP3 frame arithmetic, short-frame padding.
//!
//! One-shot evidence for the fix lane (no `#[test]`; the test tree is frozen):
//!
//! * `clamp_pitch` over NaN/±Inf/zero/negative/out-of-range/min/max;
//! * `mp3_window_span` at the window edges and at `u64::MAX` (the values a
//!   corrupted phase used to reach);
//! * `probe_mp3` header re-derivation on a shipped music track (layer, sample
//!   rate, frame length, samples per frame, bit-reservoir reference);
//! * a real MP3 played at `pitch = 1.0` and at extreme pitches, with the
//!   rendered-frame count compared to `declared_frames / step`;
//! * a truncated MP3 whose declared frame count exceeds what the data decodes:
//!   the mixer must pad with silence, log `AUDIO_SHORT_FRAME` once, and never
//!   mix an unexplained gap or panic;
//! * the ambient PCM budget constant.
//!
//! Usage: `h2-cargo run -p wc3-audio --bin audio_pitch_probe`

use std::path::PathBuf;
use std::sync::Arc;

use wc3_audio::engine::{AudioConfig, AudioEngine, DEFAULT_CHANNEL_VOICES};
use wc3_audio::game_audio::DEFAULT_AMBIENT_PCM_BUDGET;
use wc3_audio::mp3::{mp3_window_span, probe_mp3, Mp3Stream};
use wc3_audio::{clamp_pitch, AudioStats, PitchBound, PlayParams, MAX_PITCH, MIN_PITCH};

const OUT_RATE: u32 = 48_000;
const FADE_FRAMES: usize = (OUT_RATE as usize) * 5 / 1000; // 5 ms anti-click ramp

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: OUT_RATE,
        channels: 2,
        block_frames: 512,
        max_voices: 8,
        channel_voices: DEFAULT_CHANNEL_VOICES,
        master_gain: 1.0,
    }
}

/// Render one frame at a time until the voice pool is empty. Returns the number
/// of output frames and the final stats snapshot.
fn render_until_finished(engine: &mut AudioEngine, cap: usize) -> (usize, AudioStats) {
    let mut frame = [0.0f32; 2];
    engine.render(&mut frame);
    let mut frames = 1usize;
    while engine.stats().active_voices > 0 {
        assert!(frames < cap, "voice did not finish within {cap} output frames");
        engine.render(&mut frame);
        frames += 1;
    }
    (frames, engine.stats())
}

fn expected_out_frames(total_source_frames: u64, source_rate: u32, pitch: f32) -> usize {
    let step = source_rate as f64 / OUT_RATE as f64 * pitch as f64;
    (total_source_frames as f64 / step).ceil() as usize + FADE_FRAMES + 1
}

fn play_case(label: &str, bytes: Arc<[u8]>, pitch: f32) {
    let stream = Mp3Stream::new(bytes.clone()).expect("stream open");
    let total = stream.total_frames();
    let rate = stream.sample_rate();
    let (applied, bound) = clamp_pitch(pitch);
    let mut engine = AudioEngine::headless(config());
    engine
        .play_mp3(bytes, PlayParams { pitch, ..Default::default() })
        .expect("play_mp3");
    let (frames, stats) = render_until_finished(&mut engine, 4_000_000);
    let expected = total.map(|total| expected_out_frames(total, rate, applied));
    let delta = expected.map(|expected| frames as i64 - expected as i64);
    println!(
        "PLAY case={label} requested={pitch:?} bound={bound:?} applied={applied:?} source_rate={rate} declared_frames={total:?} out_frames={frames} expected={expected:?} delta={delta:?} pitches_bounded={} short_frames={}",
        stats.pitches_bounded, stats.short_frames
    );
    if let Some(delta) = delta {
        assert!(
            delta.abs() <= 3,
            "case {label}: rendered {frames} frames, expected {expected:?} (delta {delta})"
        );
    }
    assert_eq!(stats.pitches_bounded, u64::from(bound != PitchBound::Unchanged));
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    let music = root.join("Sound/Music/mp3Music/Mainscreen.mp3");
    let voice = root.join("Sound/Dialogue/UndeadCampaign/Undead01/U01Arthas28.mp3");

    // 1. Pitch bounds: every input class named by the audit.
    let cases = [
        1.0f32,
        MIN_PITCH,
        MAX_PITCH,
        0.1,
        9.0,
        1.0e30,
        0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    for pitch in cases {
        let (applied, bound) = clamp_pitch(pitch);
        println!("PITCH requested={pitch:?} applied={applied:?} bound={bound:?}");
        assert!(applied.is_finite() && applied > 0.0);
        assert!((MIN_PITCH..=MAX_PITCH).contains(&applied));
        let expected_bound = if !pitch.is_finite() || pitch <= 0.0 {
            PitchBound::Rejected
        } else if pitch < MIN_PITCH || pitch > MAX_PITCH {
            PitchBound::Clamped
        } else {
            PitchBound::Unchanged
        };
        assert_eq!(bound, expected_bound, "pitch {pitch:?}");
        if bound != PitchBound::Unchanged {
            assert_eq!(applied, if expected_bound == PitchBound::Rejected { 1.0 } else { pitch.clamp(MIN_PITCH, MAX_PITCH) });
        }
    }

    // 2. Frame-window arithmetic: edges, jumps, and the u64 extremes.
    let windows = [
        (0u64, 8192usize, 0u64),
        (0, 8192, 8190),
        (0, 8192, 8191),
        (0, 8192, 8192),
        (0, 8192, u64::MAX),
        (u64::MAX - 100, 8192, u64::MAX),
        (5, 8192, 4),
        (0, 1, 0),
        (0, 0, 0),
    ];
    for (start, frames, abs) in windows {
        println!("WINDOW start={start} frames={frames} abs={abs} -> {:?}", mp3_window_span(start, frames, abs));
    }
    assert_eq!(mp3_window_span(0, 8192, 8190), Some((8190, 8191)));
    assert_eq!(mp3_window_span(0, 8192, 8191), None);
    assert_eq!(mp3_window_span(0, 8192, u64::MAX), None);
    assert_eq!(mp3_window_span(u64::MAX - 100, 8192, u64::MAX), Some((100, 101)));
    assert_eq!(mp3_window_span(5, 8192, 4), None);

    // 3. Real music headers: layer/rate/frame-length/samples/reservoir.
    let music_bytes = Arc::<[u8]>::from(std::fs::read(&music).expect("music file"));
    let info = probe_mp3(&music_bytes).expect("probe music");
    println!(
        "MP3_INFO file={} bytes={} layer={} sample_rate={} channels={} frames={} samples_per_channel={} duration_ms={} bitrate_avg={} bitrate_min={} bitrate_max={} first_frame_offset={} vbr_tag={} reservoir_max={}",
        music.display(),
        music_bytes.len(),
        info.layer,
        info.sample_rate,
        info.channels,
        info.frames,
        info.samples_per_channel,
        info.duration_ms,
        info.bitrate_avg_kbps,
        info.bitrate_min_kbps,
        info.bitrate_max_kbps,
        info.first_frame_offset,
        info.has_vbr_tag,
        info.reservoir_max_bytes
    );
    assert_eq!(info.layer, 3);
    assert_eq!(info.sample_rate, 44_100);
    assert_eq!(info.channels, 2);
    assert!(info.frames > 1000);
    assert_eq!(
        info.samples_per_channel,
        info.frames as u64 * 1152,
        "MPEG-1 Layer III is 1152 samples per frame"
    );
    assert!(info.reservoir_max_bytes <= 511, "MPEG-1 reservoir is 9 bits");

    // 4. Normal playback: pitch 1.0, audible output, rate 1:1.
    let mut engine = AudioEngine::headless(config());
    engine
        .play_mp3(music_bytes.clone(), PlayParams { pitch: 1.0, ..Default::default() })
        .expect("play music");
    let mut out = vec![0.0f32; 512 * 2];
    let mut energy = 0.0f64;
    let mut samples = 0usize;
    for _ in 0..200 {
        let frames = engine.render(&mut out);
        energy += out.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>();
        samples += frames * 2;
    }
    let rms = (energy / samples as f64).sqrt();
    let stats = engine.stats();
    println!(
        "PLAY_NORMAL pitch=1.0 bound={:?} blocks=200 out_frames={} rms={rms:.6} pitches_bounded={} active={}",
        clamp_pitch(1.0).1,
        stats.frames_rendered,
        stats.pitches_bounded,
        stats.active_voices
    );
    assert!(rms > 0.001, "pitch 1.0 must produce audio, rms={rms}");
    assert_eq!(stats.pitches_bounded, 0, "pitch 1.0 must not be adjusted");

    // 5. Extreme pitch on a short real voice: clamped, logged, rate bounded.
    let voice_bytes = Arc::<[u8]>::from(std::fs::read(&voice).expect("voice file"));
    play_case("inf", voice_bytes.clone(), f32::INFINITY);
    play_case("nan", voice_bytes.clone(), f32::NAN);
    play_case("negative", voice_bytes.clone(), -3.0);
    play_case("huge", voice_bytes.clone(), 1.0e30);
    play_case("max", voice_bytes.clone(), MAX_PITCH);

    // 6. Short frame: declare more audio than the bytes can decode.
    let cut = voice_bytes.len() * 6 / 10;
    let truncated = Arc::<[u8]>::from(&voice_bytes[..cut]);
    let declared = Mp3Stream::new(truncated.clone()).expect("truncated stream").total_frames();
    let mut engine = AudioEngine::headless(config());
    engine
        .play_mp3(truncated.clone(), PlayParams { pitch: 1.0, ..Default::default() })
        .expect("play truncated");
    let (frames, stats) = render_until_finished(&mut engine, 4_000_000);
    // After the voice is gone the mix must be exactly silent: the short tail
    // was padded, not filled with stale buffer contents.
    let mut tail = vec![0.0f32; 512 * 2];
    engine.render(&mut tail);
    let tail_peak = tail.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    println!(
        "SHORT_FRAME file={} bytes={cut} declared_frames={declared:?} out_frames={frames} short_frames={} out_frames_played={} tail_peak={tail_peak}",
        voice.display(),
        stats.short_frames,
        stats.frames_rendered
    );
    assert_eq!(stats.short_frames, 1, "one short-frame report per voice");
    assert_eq!(tail_peak, 0.0, "the padded tail must be digital silence");

    // 7. PCM budget stays at the documented 64 MB.
    println!("PCM_BUDGET bytes={DEFAULT_AMBIENT_PCM_BUDGET} mib={}", DEFAULT_AMBIENT_PCM_BUDGET / (1024 * 1024));
    assert_eq!(DEFAULT_AMBIENT_PCM_BUDGET, 64 * 1024 * 1024);

    println!("RESULT ok");
}
