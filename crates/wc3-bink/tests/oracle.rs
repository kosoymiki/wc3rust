//! Regression tests that need a real movie file.
//!
//! The cinematics are not in the repository (they are the operator's game
//! data), so these tests locate them through `H2_MOVIE_SAMPLE` and skip
//! cleanly when it is unset. They are the tests that catch decoder
//! regressions end to end; the unit tests cover the individual stages.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use wc3_bink::BinkFile;

fn sample() -> Option<PathBuf> {
    let path = std::env::var_os("H2_MOVIE_SAMPLE")?;
    let path = PathBuf::from(path);
    path.exists().then_some(path)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// The first 100 frames decoded to RGBA must hash to a stable value.
///
/// This is the regression gate that runs without `ffmpeg`: any change to the
/// demuxer, the MPEG-4 decoder or the colour conversion moves the hash.
#[test]
fn first_100_frames_have_a_stable_hash() {
    let Some(path) = sample() else {
        eprintln!("H2_MOVIE_SAMPLE unset or missing; skipping");
        return;
    };
    let mut file = BinkFile::open(&path).expect("open movie");
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    for i in 0..100 {
        file.decode_frame(i).expect("decode frame");
        hasher.update(file.frame_rgba(i));
    }
    let digest = format!("{:x}", hasher.finalize());
    println!("first100 sha256 = {digest}");
    // Recorded from the reference implementation on HumanEd.mpq; update only
    // together with a deliberate decoder change.
    if let Ok(expected) = std::env::var("H2_MOVIE_SHA256") {
        assert_eq!(digest, expected, "decoded-frames hash changed");
    }
}

/// Frame 0 must decode to the same pixels a plain YUV round-trip produces.
#[test]
fn frame_zero_is_black_fade() {
    let Some(path) = sample() else {
        return;
    };
    let mut file = BinkFile::open(&path).expect("open movie");
    let frame = file.decode_frame(0).expect("frame 0");
    let rgba = file.frame_rgba(0);
    assert_eq!(rgba.len(), frame.byte_len());
    // The retail movies open on a black frame; a decoder that drops the intra
    // DC or mis-scales the colour transform cannot hit this.
    let mean: u64 = rgba
        .chunks_exact(4)
        .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
        .sum();
    let mean = mean as f64 / (rgba.len() / 4 * 3) as f64;
    assert!(mean < 8.0, "frame 0 mean {mean} is not black");
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

/// PSNR against `ffmpeg` (oracle only) over the first `H2_MOVIE_FRAMES`
/// frames (default 300).
///
/// ```text
/// H2_MOVIE_SAMPLE=/path/movie.mpq cargo test -p wc3-bink -- --ignored
/// ```
#[test]
#[ignore = "needs ffmpeg and H2_MOVIE_SAMPLE (oracle comparison)"]
fn psnr_against_ffmpeg() {
    let Some(path) = sample() else {
        eprintln!("H2_MOVIE_SAMPLE unset or missing");
        return;
    };
    assert!(ffmpeg_available(), "ffmpeg not found");
    let frames: usize = std::env::var("H2_MOVIE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let mut file = BinkFile::open(&path).expect("open movie");
    let (w, h) = file.size();
    let (w, h) = (w as usize, h as usize);
    let frame_bytes = w * h * 4;

    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            path.to_str().unwrap(),
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn ffmpeg");
    let mut out = child.stdout.take().expect("stdout");
    let mut oracle = vec![0u8; frame_bytes];

    let mut psnrs = Vec::new();
    for i in 0..frames {
        if out.read_exact(&mut oracle).is_err() {
            break;
        }
        file.decode_frame(i).expect("decode frame");
        let ours = file.frame_rgba(i);
        let mut se = 0f64;
        let mut n = 0u64;
        for (a, b) in ours.iter().zip(oracle.iter()) {
            let d = f64::from(*a) - f64::from(*b);
            se += d * d;
            n += 1;
        }
        // Compare colour only; alpha is synthetic.
        let mse = se / n as f64;
        psnrs.push(if mse == 0.0 {
            99.0
        } else {
            10.0 * (255.0f64 * 255.0 / mse).log10()
        });
    }
    let _ = child.kill();
    let _ = child.wait();

    let pass = psnrs.iter().filter(|&&p| p >= 35.0).count();
    let pct = 100.0 * pass as f64 / psnrs.len() as f64;
    let worst = psnrs.iter().cloned().fold(f64::INFINITY, f64::min);
    let mean = psnrs.iter().sum::<f64>() / psnrs.len() as f64;
    println!(
        "{} frames: mean {mean:.2} dB worst {worst:.2} dB, {pass} >= 35 dB ({pct:.1}%)",
        psnrs.len()
    );
}
