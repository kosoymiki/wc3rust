//! Dump per-macroblock coding data of one decoded frame as CSV.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example mb_dump -- <movie> <frame>
//! ```
//!
//! Columns: `mbx,mby,kind,mvx,mvy,qscale` where `kind` is `I`, `S` (skipped) or
//! `R` (inter), and the motion vector is the luma vector of the first block in
//! half-pel units.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: mb_dump <movie> <frame>");
        std::process::exit(2);
    }
    let mut file = wc3_bink::BinkFile::open(&args[0]).expect("open movie");
    let index: usize = args[1].parse().expect("frame");
    file.decode_frame(index).expect("decode");
    let pic = file.picture();
    let kinds = pic.mb_kind_map();
    let mb_w = (pic.width + 15) / 16;
    let mb_h = (pic.height + 15) / 16;
    println!("mbx,mby,kind,mvx,mvy,qscale");
    for mby in 0..mb_h {
        for mbx in 0..mb_w {
            let mv = pic.mv_at(mbx * 2, mby * 2);
            println!(
                "{mbx},{mby},{},{},{},",
                kinds[mby * mb_w + mbx] as char, mv[0], mv[1]
            );
        }
    }
}
