//! Decode a movie's MP3 audio with the crate's own decoder and compare the
//! PCM against an `ffmpeg -f s16le` oracle.
//!
//! ```text
//! ffmpeg -v error -i audio.mp3 -f s16le -acodec pcm_s16le - > oracle.s16
//! scripts/h2-cargo run -p wc3-bink --example mp3_diff -- <movie> <oracle.s16> [frames]
//! ```
//!
//! The encoder delay (and the LAME gapless tag that ffmpeg honours) shifts the
//! oracle by a few hundred samples, so the tool first searches for the sample
//! offset that minimises the error, then reports the maximum absolute
//! difference over the aligned region — the acceptance metric is <= 2 LSB.

use std::io::Read;

use wc3_bink::mp3::Mp3Decoder;

fn read_s16(path: &str) -> Vec<i16> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .and_then(|mut f| f.read_to_end(&mut raw))
        .expect("read oracle");
    raw.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: mp3_diff <movie> <oracle.s16> [max_frames]");
        std::process::exit(2);
    }
    let movie = &args[0];
    let oracle = read_s16(&args[1]);
    let max_frames: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);

    let file = wc3_bink::BinkFile::open(movie).expect("open movie");
    let mut decoder = Mp3Decoder::new();
    let mut ours: Vec<i16> = Vec::new();
    let mut stream: Vec<u8> = Vec::new();
    for chunk in file.avi().audio_chunks() {
        stream.extend_from_slice(chunk);
    }
    let mut rest: &[u8] = &stream;
    let mut frames = 0usize;
    while !rest.is_empty() && frames < max_frames {
        match decoder.decode(rest, &mut ours) {
            Ok(used) if used > 0 => {
                rest = &rest[used..];
                frames += 1;
            }
            Ok(_) => break,
            Err(err) => {
                eprintln!("stop after {frames} frames: {err}");
                break;
            }
        }
    }
    println!(
        "frames={frames} channels={} rate={} samples={} oracle_samples={}",
        decoder.channels,
        decoder.sample_rate,
        ours.len(),
        oracle.len()
    );

    if let Ok(path) = std::env::var("H2_MP3_PCM") {
        let mut raw = Vec::with_capacity(ours.len() * 2);
        for s in &ours {
            raw.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(path, raw).expect("write our pcm");
    }
    {
        let rms = |v: &[i16]| -> f64 {
            (v.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / v.len().max(1) as f64)
                .sqrt()
        };
        println!("rms ours={:.2} oracle={:.2}", rms(&ours), rms(&oracle));
    }
    let nch = decoder.channels.max(1);
    // Coarse offset search on a mid-stream window of 1 s.
    let probe_start = ours.len().min(oracle.len()).saturating_sub(44_100 * nch * 4) / 4;
    let probe_len = 44_100 * nch;
    let mut best = (0i64, i64::MAX);
    for off in -3000i64..=3000 {
        let mut cost = 0i64;
        let mut n = 0usize;
        for i in (0..probe_len).step_by(7) {
            let oi = probe_start + i;
            let fi = oi as i64 + off;
            if fi < 0 || fi as usize >= oracle.len() || oi >= ours.len() {
                break;
            }
            cost += (i64::from(ours[oi]) - i64::from(oracle[fi as usize])).abs();
            n += 1;
        }
        if n > 100 && (cost / n as i64) < best.1 {
            best = (off, cost / n as i64);
        }
    }
    let off = best.0;
    println!("best_offset={off} mean_abs_error={}", best.1);

    let mut max_diff = 0i32;
    let mut over2 = 0usize;
    let mut compared = 0usize;
    let mut first_bad: Option<(usize, i16, i16)> = None;
    for i in 0..ours.len() {
        let fi = i as i64 + off;
        if fi < 0 || fi as usize >= oracle.len() {
            continue;
        }
        let d = (i32::from(ours[i]) - i32::from(oracle[fi as usize])).abs();
        compared += 1;
        if d > max_diff {
            max_diff = d;
        }
        if d > 2 {
            over2 += 1;
            if first_bad.is_none() {
                first_bad = Some((i, ours[i], oracle[fi as usize]));
            }
        }
    }
    // Best-fit scalar gain between ours and the oracle, and the residual after
    // removing it: distinguishes a pure level error from a shape error.
    let (mut num, mut den, mut oo, mut rr) = (0f64, 0f64, 0f64, 0f64);
    for i in (0..ours.len()).step_by(3) {
        let fi = i as i64 + off;
        if fi < 0 || fi as usize >= oracle.len() {
            continue;
        }
        let a = f64::from(ours[i]);
        let b = f64::from(oracle[fi as usize]);
        num += a * b;
        den += a * a;
        oo += b * b;
    }
    let gain = if den > 0.0 { num / den } else { 0.0 };
    let corr = if den > 0.0 && oo > 0.0 { num / (den.sqrt() * oo.sqrt()) } else { 0.0 };
    for i in (0..ours.len()).step_by(3) {
        let fi = i as i64 + off;
        if fi < 0 || fi as usize >= oracle.len() {
            continue;
        }
        let d = f64::from(ours[i]) * gain - f64::from(oracle[fi as usize]);
        rr += d * d;
    }
    println!("gain={gain:.6} correlation={corr:.6}");
    println!(
        "compared={compared} max_abs_diff={max_diff} samples_over_2lsb={over2} ({:.4}%)",
        100.0 * over2 as f64 / compared.max(1) as f64
    );
    if let Some((i, a, b)) = first_bad {
        println!("first_bad sample {i}: ours={a} oracle={b}");
    }
    std::process::exit(if over2 * 10_000 <= compared && max_diff <= 2 {
        0
    } else {
        1
    });
}
