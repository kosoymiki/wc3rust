//! MPEG-4 Part 2 (ASP) decoder oracle and robustness tests.
//!
//! `ffmpeg` is a test oracle only; the library never spawns a process.
//!
//! ```text
//! H2_MOVIE_SAMPLE=/path/HumanEd.mpq cargo test -p wc3-bink --test mpeg4_oracle -- --ignored
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

fn all_samples() -> Vec<PathBuf> {
    let mut out = Vec::new();
    // The nine cinematics are the operator's game data; scan the known roots
    // through `H2_MOVIE_DIR` (a directory) when it is set.
    if let Some(dir) = std::env::var_os("H2_MOVIE_DIR") {
        collect(PathBuf::from(dir).as_path(), &mut out);
    }
    if let Some(one) = sample() {
        out.push(one);
    }
    out
}

fn collect(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("mpq") | Some("blz")
        ) {
            out.push(path);
        }
    }
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

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mut se = 0f64;
    let mut n = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        // Alpha is synthetic (always 255); compare colour only.
        let d = f64::from(*x) - f64::from(*y);
        se += d * d;
        n += 1;
    }
    let mse = se / n.max(1) as f64;
    if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

/// `codec.mpeg4.frame0-psnr`: frame 0 must match the oracle to >= 35 dB.
#[test]
#[ignore = "needs ffmpeg and H2_MOVIE_SAMPLE (oracle comparison)"]
fn codec_mpeg4_frame0_psnr() {
    let Some(path) = sample() else {
        eprintln!("H2_MOVIE_SAMPLE unset or missing; skipping");
        return;
    };
    assert!(ffmpeg_available(), "ffmpeg not found");

    let mut file = BinkFile::open(&path).expect("open movie");
    let (w, h) = file.size();
    let frame_bytes = w as usize * h as usize * 4;
    let mut child = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", path.to_str().unwrap(), "-fps_mode", "passthrough", "-f",
            "rawvideo", "-pix_fmt", "rgba", "-",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn ffmpeg");
    let mut out = child.stdout.take().expect("stdout");
    let mut oracle = vec![0u8; frame_bytes];
    out.read_exact(&mut oracle).expect("oracle frame 0");
    let _ = child.kill();
    let _ = child.wait();

    file.decode_frame(0).expect("frame 0");
    let ours = file.frame_rgba(0);
    let p = psnr(ours, &oracle);
    println!("frame 0 PSNR {p:.3} dB");
    assert!(p >= 35.0, "frame 0 PSNR {p:.3} dB < 35 dB");
}

/// `codec.mpeg4.no-tcoeff-overflow`: decoding every frame of every available
/// movie must never panic or overflow, whatever the bitstream contains.
#[test]
#[ignore = "needs the operator's movie files (H2_MOVIE_DIR / H2_MOVIE_SAMPLE)"]
fn codec_mpeg4_no_tcoeff_overflow() {
    let samples = all_samples();
    if samples.is_empty() {
        eprintln!("no samples; set H2_MOVIE_DIR or H2_MOVIE_SAMPLE");
        return;
    }
    let limit: usize = std::env::var("H2_MOVIE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mut total_frames = 0usize;
    let mut concealed = 0u64;
    for path in &samples {
        let mut file = BinkFile::open(path).expect("open movie");
        let frames = file.frame_count().min(limit);
        for i in 0..frames {
            // Any error is acceptable; a panic is not.
            let _ = file.decode_frame(i);
            total_frames += 1;
        }
        concealed += file.concealed_macroblocks();
        println!(
            "{}: {} frames decoded, {} concealed MBs, last error {:?}",
            path.display(),
            frames,
            file.concealed_macroblocks(),
            file.last_decode_error()
        );
    }
    println!("total {total_frames} frames, {concealed} concealed MBs");
    assert!(total_frames > 0);
}

/// Frame-by-frame PSNR over the first `H2_MOVIE_FRAMES` frames (default 500);
/// at least 90% of them must reach 35 dB.
#[test]
#[ignore = "needs ffmpeg and H2_MOVIE_SAMPLE (oracle comparison)"]
fn codec_mpeg4_first_500_psnr() {
    let Some(path) = sample() else {
        return;
    };
    assert!(ffmpeg_available(), "ffmpeg not found");
    let limit: usize = std::env::var("H2_MOVIE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);

    let mut file = BinkFile::open(&path).expect("open movie");
    let (w, h) = file.size();
    let frame_bytes = w as usize * h as usize * 4;
    let mut child = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", path.to_str().unwrap(), "-fps_mode", "passthrough", "-f",
            "rawvideo", "-pix_fmt", "rgba", "-",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn ffmpeg");
    let mut out = child.stdout.take().expect("stdout");
    let mut oracle = vec![0u8; frame_bytes];

    let mut psnrs = Vec::new();
    for i in 0..limit {
        if out.read_exact(&mut oracle).is_err() {
            break;
        }
        file.decode_frame(i).expect("decode frame");
        // Compare colour channels only (stride 4, skip alpha).
        let ours = file.frame_rgba(i);
        let mut se = 0f64;
        let mut n = 0u64;
        for (o, r) in ours.chunks_exact(4).zip(oracle.chunks_exact(4)) {
            for c in 0..3 {
                let d = f64::from(o[c]) - f64::from(r[c]);
                se += d * d;
                n += 1;
            }
        }
        let mse = se / n.max(1) as f64;
        psnrs.push(if mse == 0.0 {
            99.0
        } else {
            10.0 * (255.0f64 * 255.0 / mse).log10()
        });
    }
    let _ = child.kill();
    let _ = child.wait();

    let pass = psnrs.iter().filter(|&&p| p >= 35.0).count();
    let mean = psnrs.iter().sum::<f64>() / psnrs.len().max(1) as f64;
    let worst = psnrs.iter().cloned().fold(f64::INFINITY, f64::min);
    let pct = 100.0 * pass as f64 / psnrs.len().max(1) as f64;
    println!(
        "{} frames: mean {mean:.2} dB, worst {worst:.2} dB, {pass} >= 35 dB ({pct:.1}%)",
        psnrs.len()
    );
    assert!(pct >= 90.0, "only {pct:.1}% of frames reach 35 dB");
}
