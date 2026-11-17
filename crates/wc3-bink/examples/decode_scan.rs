//! Decode every frame of a movie and report decode errors and concealment.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example decode_scan -- <movie> [max_frames]
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: decode_scan <movie> [max_frames]");
        std::process::exit(2);
    }
    let mut file = wc3_bink::BinkFile::open(&args[0]).expect("open movie");
    let max: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let total = file.frame_count().min(max);
    let mut errors = Vec::new();
    for i in 0..total {
        if let Err(e) = file.decode_frame(i) {
            errors.push((i, format!("{e}")));
        }
    }
    println!(
        "{}: {} frames, {} hard errors, {} concealed MBs, last error {:?}",
        args[0],
        total,
        errors.len(),
        file.concealed_macroblocks(),
        file.last_decode_error()
    );
    for (i, e) in errors.iter().take(5) {
        println!("  frame {i}: {e}");
    }
}
