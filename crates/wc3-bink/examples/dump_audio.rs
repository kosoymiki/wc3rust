//! Concatenate the container's MP3 audio chunks into a raw `.mp3` stream.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example dump_audio -- <movie> <out.mp3>
//! ```
//!
//! The chunks are already complete MPEG audio frames in a BLZ0/AVI file, so
//! the concatenation is a decodable elementary stream: it is both the input
//! for the own decoder and the oracle comparison (`ffmpeg -f s16le`).

use std::io::Write;

use wc3_bink::BinkFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: dump_audio <movie> <out.mp3>");
        std::process::exit(2);
    }
    let file = BinkFile::open(&args[0]).expect("open movie");
    let chunks = file.avi().audio_chunks();
    let mut total = 0usize;
    let mut count = 0usize;
    let mut out = std::fs::File::create(&args[1]).expect("create output");
    for chunk in chunks {
        out.write_all(chunk).expect("write chunk");
        total += chunk.len();
        count += 1;
    }
    let info = file.audio_info();
    println!(
        "chunks={count} bytes={total} info={:?}",
        info.map(|a| (a.format_tag, a.channels, a.sample_rate, a.length))
    );
}
