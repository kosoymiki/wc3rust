//! MP3 (MPEG-1 Layer III) oracle tests.
//!
//! The own decoder is compared against `ffmpeg -f s16le`. `ffmpeg` is a test
//! oracle only: it never enters the library, and `cargo tree -p wc3-bink`
//! stays dependency-free.
//!
//! ```text
//! H2_MOVIE_SAMPLE=/path/HumanEd.mpq cargo test -p wc3-bink --test mp3_oracle -- --ignored
//! ```

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use wc3_bink::BinkFile;

fn sample() -> Option<PathBuf> {
    let path = std::env::var_os("H2_MOVIE_SAMPLE")?;
    let path = PathBuf::from(path);
    path.exists().then_some(path)
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Decode the movie's audio track with `ffmpeg` to interleaved s16le.
fn oracle_pcm(path: &PathBuf) -> Vec<i16> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            path.to_str().expect("utf-8 path"),
            "-map",
            "0:a:0",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ffmpeg");
    let mut raw = Vec::new();
    child
        .stdout
        .take()
        .expect("stdout")
        .read_to_end(&mut raw)
        .expect("read ffmpeg");
    let _ = child.wait();
    raw.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

#[test]
#[ignore = "needs ffmpeg and H2_MOVIE_SAMPLE (oracle comparison)"]
fn codec_mp3_delta() {
    let Some(path) = sample() else {
        eprintln!("H2_MOVIE_SAMPLE unset or missing; skipping");
        return;
    };
    assert!(ffmpeg_available(), "ffmpeg not found");

    let mut file = BinkFile::open(&path).expect("open movie");
    let rate = file.audio_sample_rate();
    assert_eq!(rate, 44_100, "WC3 cinematics are 44.1 kHz");
    let ours_full = {
        let pcm = file.audio_pcm().expect("decode audio");
        pcm.to_vec()
    };
    let oracle = oracle_pcm(&path);
    assert!(!oracle.is_empty(), "oracle produced no samples");
    assert!(!ours_full.is_empty(), "decoder produced no samples");

    // The encoder delay (and the LAME gapless tag ffmpeg honours) shifts the
    // oracle, so align first, then compare.
    let channels = 2usize;
    let probe_len = (rate as usize) * channels; // one second
    let probe_start = (ours_full.len().saturating_sub(probe_len * 4)) / 4 / channels * channels;
    let mut best = (0i64, u64::MAX);
    for off in -(rate as i64)..=(rate as i64) {
        let mut cost = 0u64;
        let mut n = 0u64;
        for i in (0..probe_len).step_by(17) {
            let oi = probe_start + i;
            let fi = oi as i64 + off;
            if oi >= ours_full.len() || fi < 0 || fi as usize >= oracle.len() {
                break;
            }
            cost += u64::from(ours_full[oi].abs_diff(oracle[fi as usize]));
            n += 1;
        }
        if n > 1000 && cost / n < best.1 {
            best = (off, cost / n);
        }
    }
    let off = best.0;

    let mut max_diff = 0i32;
    let mut over = 0usize;
    let mut compared = 0usize;
    for i in 0..ours_full.len() {
        let fi = i as i64 + off;
        if fi < 0 || fi as usize >= oracle.len() {
            continue;
        }
        let d = (i32::from(ours_full[i]) - i32::from(oracle[fi as usize])).abs();
        max_diff = max_diff.max(d);
        if d > 2 {
            over += 1;
        }
        compared += 1;
    }
    println!(
        "mp3: {} frames, {} samples, offset {off}, {compared} compared, max |delta| = {max_diff} LSB, {over} samples over 2 LSB",
        file.avi().audio_chunk_count(),
        ours_full.len()
    );
    assert!(compared > 441_000, "not enough audio compared");
    assert!(
        over == 0,
        "{over} samples exceed the 2 LSB tolerance (max {max_diff})"
    );
}

#[test]
#[ignore = "needs H2_MOVIE_SAMPLE"]
fn codec_mp3_duration_matches_the_container() {
    let Some(path) = sample() else {
        return;
    };
    let mut file = BinkFile::open(&path).expect("open movie");
    let rate = file.audio_sample_rate() as usize;
    let duration = file.duration_secs();
    let len = file.audio_pcm().expect("decode audio").len();
    let channels = file.audio_info().map_or(2, |a| a.channels as usize);
    let seconds = len as f64 / (rate * channels) as f64;
    println!("audio {seconds:.3} s vs video {duration:.3} s");
    assert!(
        (seconds - duration).abs() < 1.0,
        "audio {seconds:.3}s vs video {duration:.3}s"
    );
}
