//! Probe the own decoder over a shipped movie (`movie_decoder` feature).
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --features movie_decoder --example movie_probe -- HumanEd
//! ```
//!
//! Prints the container size, frame count, declared duration, then decodes three
//! frames (first, middle, last) and prints their RGBA byte length and a 64-bit
//! FNV-1a hash. The hash is the reproducible receipt that the bytes came out of
//! our decoder; it is not an oracle comparison (that lives in `wc3-bink` tests).

#[cfg(feature = "movie_decoder")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use wc3_game::cinematic::{MovieDecoder, MovieLibrary, bink_decoder::OwnDecoder};

    let name = std::env::args().nth(1).unwrap_or_else(|| "HumanEd".to_string());
    let library = MovieLibrary::installed();
    let movie = library
        .find(&name)
        .ok_or_else(|| format!("MOVIE_MISSING {name}"))?
        .clone();
    println!(
        "file={} bytes={} roots_scanned={}",
        movie.path.display(),
        movie.bytes,
        library.roots.len()
    );
    let mut decoder = OwnDecoder::open(&movie.path)?;
    let (width, height) = decoder.size();
    println!(
        "size={width}x{height} fps={:.3} duration={:.3}s audio_rate={}",
        decoder.fps(),
        decoder.duration_secs(),
        decoder.sample_rate()
    );
    for index in [0usize, 1, 100] {
        match decoder.frame(index) {
            Ok(rgba) => {
                let mut hash = 0xcbf2_9ce4_8422_2325u64;
                for byte in &rgba {
                    hash ^= u64::from(*byte);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
                println!("frame={index} bytes={} fnv1a={hash:016x}", rgba.len());
            }
            Err(err) => println!("frame={index} error={err}"),
        }
    }
    let mut audio = Vec::new();
    match decoder.audio(&mut audio) {
        Ok(()) => println!("audio_samples={}", audio.len()),
        Err(err) => println!("audio_error={err}"),
    }
    Ok(())
}

#[cfg(not(feature = "movie_decoder"))]
fn main() {
    eprintln!("rebuild with --features movie_decoder to run the decoder probe");
}
