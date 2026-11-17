//! Print the container's video chunk sizes around a frame index.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example chunk_sizes -- <movie> <from> <to>
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: chunk_sizes <movie> <from> <to>");
        std::process::exit(2);
    }
    let file = wc3_bink::BinkFile::open(&args[0]).expect("open movie");
    let from: usize = args[1].parse().expect("from");
    let to: usize = args[2].parse().expect("to");
    let avi = file.avi();
    for i in from..=to.min(file.frame_count().saturating_sub(1)) {
        let payload = avi.video_frame(i).unwrap_or(&[]);
        let head: String = payload
            .iter()
            .take(6)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join("");
        println!("frame {i:5} len={:6} head={head}", payload.len());
    }
    println!("declared frames = {}", file.frame_count());
}
