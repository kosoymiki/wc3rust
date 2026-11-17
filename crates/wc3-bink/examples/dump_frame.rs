//! Decode one frame to raw RGBA (for oracle comparison and visual checks).
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example dump_frame -- <file> <index> <out.rgba>
//! ```

use std::io::Write;

use wc3_bink::BinkFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: dump_frame <movie.mpq|.blz> <frame-index> <out.rgba>");
        std::process::exit(2);
    }
    let path = &args[0];
    let index: usize = args[1].parse().expect("frame index");
    let out = &args[2];

    // `--at <index>` decodes frame 0 for the VOL and then only that payload
    // (valid for I-frames, which are self-contained).
    if let Some(pos) = args.iter().position(|a| a == "--at") {
        let target: usize = args[pos + 1].parse().expect("index");
        let mut file = BinkFile::open(path).expect("open");
        file.decode_frame(0).expect("frame 0");
        let frame = file.decode_frame_isolated(target).expect("isolated frame");
        let rgba = file.frame_rgba(target);
        std::fs::File::create(out).expect("create").write_all(rgba).expect("write");
        println!("decoded isolated frame {} ({}x{})", frame.index, frame.width, frame.height);
        return;
    }

    let mut file = match BinkFile::open(path) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("{path}: {err}");
            std::process::exit(1);
        }
    };
    let frame = match file.decode_frame(index) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("frame {index}: {err}");
            std::process::exit(1);
        }
    };
    let rgba = file.frame_rgba(index);
    assert_eq!(rgba.len(), frame.byte_len(), "rgba length");
    let mut fh = std::fs::File::create(out).expect("create output");
    fh.write_all(rgba).expect("write output");
    println!(
        "decoded frame {} ({}x{}) -> {} ({} bytes)",
        frame.index, frame.width, frame.height, out, rgba.len()
    );
}
