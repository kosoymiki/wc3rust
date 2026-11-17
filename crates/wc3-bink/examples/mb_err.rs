//! Per-macroblock error map against the `ffmpeg` oracle.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example mb_err -- <movie> <frame> [frame...]
//! ```
//!
//! For each requested frame this prints the 16x16-block mean absolute error
//! (over RGB, ignoring alpha) as a compact grid, so the first block that
//! diverges can be located in the picture instead of guessed at.

use std::io::Read;
use std::process::{Command, Stdio};

use wc3_bink::BinkFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: mb_err <movie> <frame> [frame...]");
        std::process::exit(2);
    }
    let path = &args[0];
    let frames: Vec<usize> = args[1..].iter().filter_map(|s| s.parse().ok()).collect();
    let last = *frames.iter().max().expect("frames");

    let mut file = BinkFile::open(path).expect("open movie");
    let (w, h) = file.size();
    let (w, h) = (w as usize, h as usize);
    let frame_bytes = w * h * 4;

    let mut child = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", path, "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt",
            "rgba", "-",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn ffmpeg");
    let mut out = child.stdout.take().expect("stdout");
    let mut oracle = vec![0u8; frame_bytes];
    let mut want = frames.clone();
    want.sort_unstable();

    for i in 0..=last {
        if out.read_exact(&mut oracle).is_err() {
            eprintln!("oracle ended at {i}");
            break;
        }
        if file.decode_frame(i).is_err() {
            eprintln!("decode error at {i}");
            break;
        }
        if !want.contains(&i) {
            continue;
        }
        let ours = file.frame_rgba(i);
        println!("frame {i}:");
        let mbs_x = w.div_ceil(16);
        let mut worst = (0usize, 0usize, 0f64);
        for mby in 0..h.div_ceil(16) {
            let mut line = String::new();
            for mbx in 0..mbs_x {
                let mut sum = 0f64;
                let mut n = 0f64;
                for y in mby * 16..((mby + 1) * 16).min(h) {
                    for x in mbx * 16..((mbx + 1) * 16).min(w) {
                        let o = y * w + x;
                        for c in 0..3 {
                            let a = i32::from(ours[o * 4 + c]);
                            let b = i32::from(oracle[o * 4 + c]);
                            sum += f64::from((a - b).abs());
                            n += 1.0;
                        }
                    }
                }
                let v = if n > 0.0 { sum / n } else { 0.0 };
                if v > worst.2 {
                    worst = (mbx, mby, v);
                }
                line.push_str(&format!("{v:4.1} "));
            }
            println!("  y{mby:2} {line}");
        }
        println!("  worst mb({},{}) mae={:.2}", worst.0, worst.1, worst.2);
    }
    let _ = child.kill();
    let _ = child.wait();
}
