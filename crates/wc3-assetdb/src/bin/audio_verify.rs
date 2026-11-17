//! Compare our decoders against an independent reference decode.
//!
//! Usage:
//!   audio_verify decode <input> --raw <reference.f32le> [--max-samples N]
//!   audio_verify header <input>
//!
//! The reference file is raw interleaved f32 little-endian produced by another
//! decoder (ffmpeg: `-f f32le -acodec pcm_f32le`). The tool reports sample count,
//! max absolute difference and RMS difference, which is the evidence that the
//! WAV/MP3 decode path is numerically correct rather than merely self-consistent.

use std::path::PathBuf;

use wc3_audio::{decode_mp3_bounded, decode_wav, probe_mp3, probe_wav};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: audio_verify <decode|header> <input> [--raw ref.f32le] [--max-samples N]".into());
    }
    let mode = args[0].as_str();
    let input = PathBuf::from(&args[1]);
    let mut raw: Option<PathBuf> = None;
    let mut max_samples = usize::MAX;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--raw" => {
                i += 1;
                raw = Some(PathBuf::from(&args[i]));
            }
            "--max-samples" => {
                i += 1;
                max_samples = args[i].parse()?;
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }
    let bytes = std::fs::read(&input)?;
    let lower = input.to_string_lossy().to_ascii_lowercase();
    let is_mp3 = lower.ends_with(".mp3");

    if mode == "header" {
        if is_mp3 {
            let info = probe_mp3(&bytes)?;
            println!(
                "{{\"file\":\"{}\",\"kind\":\"mp3\",\"sample_rate\":{},\"channels\":{},\"frames\":{},\"duration_ms\":{},\"bitrate_kbps\":{}}}",
                input.display(),
                info.sample_rate,
                info.channels,
                info.frames,
                info.duration_ms,
                info.bitrate_avg_kbps
            );
        } else {
            let info = probe_wav(&bytes)?;
            println!(
                "{{\"file\":\"{}\",\"kind\":\"wav\",\"codec\":\"{}\",\"sample_rate\":{},\"channels\":{},\"frames\":{},\"duration_ms\":{}}}",
                input.display(),
                info.codec.name(),
                info.sample_rate,
                info.channels,
                info.frames,
                info.duration_ms
            );
        }
        return Ok(());
    }

    let sound = if is_mp3 {
        decode_mp3_bounded(&bytes, max_samples.min(48_000 * 600))?.0
    } else {
        decode_wav(&bytes)?
    };
    let Some(raw_path) = raw else {
        return Err("--raw is required for decode mode".into());
    };
    let raw_bytes = std::fs::read(raw_path)?;
    let reference: Vec<f32> = raw_bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().expect("4")))
        .collect();
    let n = sound.samples.len().min(reference.len());
    let mut max_diff = 0.0f32;
    let mut sum_sq = 0.0f64;
    let mut ref_sq = 0.0f64;
    for i in 0..n {
        let diff = (sound.samples[i] - reference[i]).abs();
        max_diff = max_diff.max(diff);
        sum_sq += (diff as f64) * (diff as f64);
        ref_sq += (reference[i] as f64) * (reference[i] as f64);
    }
    let rms = (sum_sq / n.max(1) as f64).sqrt();
    let ref_rms = (ref_sq / n.max(1) as f64).sqrt();
    println!(
        "{{\"file\":\"{}\",\"ours_samples\":{},\"ref_samples\":{},\"compared\":{},\"max_abs_diff\":{:.8},\"rms_diff\":{:.8},\"ref_rms\":{:.6},\"snr_db\":{:.2}}}",
        input.display(),
        sound.samples.len(),
        reference.len(),
        n,
        max_diff,
        rms,
        ref_rms,
        if rms > 0.0 { 20.0 * (ref_rms / rms).log10() } else { 999.0 }
    );
    Ok(())
}
