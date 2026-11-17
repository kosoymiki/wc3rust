//! Per-frame PSNR against `ffmpeg` as an oracle.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example psnr_report -- <movie> [max_frames]
//! ```
//!
//! `ffmpeg` is used *only* here, as a reference decoder writing raw RGBA to
//! stdout; the library itself never touches it. This is the same comparison the
//! `#[ignore]`d integration test performs, with a human-readable table.

use std::io::Read;
use std::process::{Command, Stdio};

use wc3_bink::BinkFile;

fn psnr(a: &[u8], b: &[u8]) -> (f64, u8) {
    let mut se = 0f64;
    let mut n = 0u64;
    let mut max = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = i32::from(*x) - i32::from(*y);
        se += f64::from(d * d);
        n += 1;
        max = max.max(d.unsigned_abs() as u8);
    }
    let mse = se / n as f64;
    let psnr = if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    };
    (psnr, max)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: psnr_report <movie> [max_frames]");
        std::process::exit(2);
    }
    let path = &args[0];
    let max_frames: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(120);

    let mut file = BinkFile::open(path).expect("open movie");
    let (w, h) = file.size();
    let (w, h) = (w as usize, h as usize);
    let frame_bytes = w * h * 4;
    let total = file.frame_count();
    let limit = max_frames.min(total);

    let mut child = Command::new("ffmpeg")
        // `passthrough` keeps container timebase games from duplicating or
        // dropping frames, so oracle frame N really is stream frame N.
        .args([
            "-v",
            "error",
            "-i",
            path,
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

    let mut frame_psnr = Vec::with_capacity(limit);
    let mut failures = 0usize;
    for i in 0..limit {
        if out.read_exact(&mut oracle).is_err() {
            eprintln!("oracle ended at frame {i}");
            break;
        }
        match file.decode_frame(i) {
            Ok(_) => {}
            Err(err) => {
                println!("frame {i:5} DECODE ERROR {err}");
                failures += 1;
                break;
            }
        }
        let ours = file.frame_rgba(i);
        let (p, mx) = psnr(ours, &oracle);
        frame_psnr.push(p);
        if i < 12 || p < 40.0 {
            println!("frame {i:5} psnr={p:7.3} dB maxdiff={mx:3}");
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    if frame_psnr.is_empty() {
        println!("no frames compared");
        std::process::exit(1);
    }
    let mut sorted = frame_psnr.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let worst = sorted[0];
    let worst_idx = frame_psnr
        .iter()
        .position(|&p| p == worst)
        .unwrap_or(usize::MAX);
    let mean = frame_psnr.iter().sum::<f64>() / frame_psnr.len() as f64;
    let pass = frame_psnr.iter().filter(|&&p| p >= 35.0).count();
    let pct = 100.0 * pass as f64 / frame_psnr.len() as f64;
    println!(
        "\nframes={} mean={mean:.3} dB worst={worst:.3} dB @frame {worst_idx} >=35dB: {pass}/{} ({pct:.1}%) errors={failures}",
        frame_psnr.len(),
        frame_psnr.len()
    );
    std::process::exit(if pct >= 90.0 && failures == 0 { 0 } else { 1 });
}
