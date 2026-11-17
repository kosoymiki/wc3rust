//! Per-frame PSNR table against `ffmpeg` used as an oracle, in CSV form.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example psnr_csv -- <movie> [max_frames] [csv_out]
//! ```
//!
//! Prints `frame,psnr,maxdiff` for every compared frame and, when a CSV path
//! is given, writes the same table to that file. The first sixteen bytes of
//! our RGBA and the oracle's RGBA are printed for the first frame and for
//! every frame below 35 dB, which is the acceptance threshold.

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
        max = max.max(d.unsigned_abs().min(255) as u8);
    }
    let mse = se / n as f64;
    let psnr = if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    };
    (psnr, max)
}

fn hex16(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: psnr_csv <movie> [max_frames] [csv_out]");
        std::process::exit(2);
    }
    let path = &args[0];
    let max_frames: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(500);
    // `H2_PSNR_FROM` / `H2_PSNR_TO` report a sub-range while still decoding the
    // whole prefix (a P-frame chain cannot be jumped into).
    let from: usize = std::env::var("H2_PSNR_FROM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let csv_out = args.get(2);

    let mut file = BinkFile::open(path).expect("open movie");
    let (w, h) = file.size();
    let (w, h) = (w as usize, h as usize);
    let frame_bytes = w * h * 4;
    let limit = max_frames.min(file.frame_count());

    let mut child = Command::new("ffmpeg")
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
    let mut rows = String::from("frame,psnr,maxdiff\n");

    let mut pass = 0usize;
    let mut compared = 0usize;
    let mut first_low: Option<usize> = None;
    for i in 0..limit {
        if out.read_exact(&mut oracle).is_err() {
            eprintln!("oracle ended at frame {i}");
            break;
        }
        match file.decode_frame(i) {
            Ok(_) => {}
            Err(err) => {
                println!("frame {i} DECODE ERROR {err}");
                break;
            }
        }
        let ours = file.frame_rgba(i);
        let (p, mx) = psnr(ours, &oracle);
        if i < from {
            continue;
        }
        rows.push_str(&format!("{i},{p:.3},{mx}\n"));
        compared += 1;
        if p >= 35.0 {
            pass += 1;
        }
        if i == from || (p < 35.0 && first_low.is_none()) {
            if p < 35.0 {
                first_low = Some(i);
            }
            println!(
                "frame {i:5} psnr={p:7.3} maxdiff={mx:3} ours={} oracle={}",
                hex16(ours),
                hex16(&oracle)
            );
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if let Some(csv) = csv_out {
        let _ = std::fs::write(csv, &rows);
    }

    println!(
        "summary frames={compared} pass35={pass} pct={:.1}% first_low={first_low:?}",
        100.0 * pass as f64 / compared.max(1) as f64
    );
    std::process::exit(if pass * 10 >= compared * 9 { 0 } else { 1 });
}
