//! Print the container census of a Warcraft III cinematic.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example census -- <file.mpq|file.blz>
//! ```
//!
//! The retail `Movies\*.mpq` and `trailers\*.blz` files are AVI containers
//! (RIFF), not MPQ archives and not Bink files; this example shows what the
//! demuxer sees: streams, codec fourcc, frame count and payload chunk counts.

use wc3_bink::BinkFile;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: census <movie.mpq|movie.blz>");
        std::process::exit(2);
    };
    let file = match BinkFile::open(&path) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("{path}: {err}");
            std::process::exit(1);
        }
    };
    let v = file.video();
    println!("file        {path}");
    println!("container   RIFF/AVI");
    println!(
        "video       fourcc={} handler={} {}x{} bit={}",
        v.fourcc_str(),
        v.handler_str(),
        v.width,
        v.height,
        v.bit_count
    );
    println!("frames      {}", file.frame_count());
    println!(
        "rate        {}/{} = {:.3} fps, {:.2} s",
        v.rate,
        v.scale,
        v.fps(),
        file.duration_secs()
    );
    match file.audio_info() {
        Some(a) => println!(
            "audio       tag=0x{:04X} ({}), {} ch, {} Hz, {} chunks",
            a.format_tag,
            if a.is_mpeg_layer3() { "MPEG-1 Layer III" } else { "?" },
            a.channels,
            a.sample_rate,
            file.avi().audio_chunk_count()
        ),
        None => println!("audio       none"),
    }
    println!(
        "chunks      {} total ({} video, {} audio, {} idx1 entries)",
        file.avi().chunks().len(),
        file.avi().video_frame_count(),
        file.avi().audio_chunk_count(),
        file.avi().idx1().len()
    );
}
