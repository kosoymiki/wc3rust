//! Print the macroblock kind map (`I` intra, `S` skipped, `R` inter with
//! residual, `.` inter without) for one decoded frame.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example mb_map -- <movie> <frame>
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: mb_map <movie> <frame>");
        std::process::exit(2);
    }
    let path = &args[0];
    let index: usize = args[1].parse().expect("frame index");
    let mut file = wc3_bink::BinkFile::open(path).expect("open movie");
    file.decode_frame(index).expect("decode frame");
    let pic = file.picture();
    let map = pic.mb_kind_map();
    println!("frame {index} {}x{} mbs", pic.width, pic.height);
    let mb_w = (pic.width + 15) / 16;
    for (i, kind) in map.iter().enumerate() {
        if i % mb_w == 0 {
            print!("y{:2} ", i / mb_w);
        }
        print!("{}", *kind as char);
        if i % mb_w == mb_w - 1 {
            println!();
        }
    }
}
