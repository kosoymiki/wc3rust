//! Print the VOL header fields of a movie.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example vol_dump -- <movie>
//! ```

fn main() {
    let path = std::env::args().nth(1).expect("usage: vol_dump <movie>");
    let mut file = wc3_bink::BinkFile::open(&path).expect("open movie");
    file.decode_frame(0).expect("decode frame 0");
    println!("{:#?}", file.vol());
}
