//! Per-frame syntax statistics: which coding tools each frame uses.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example mb_stats -- <movie> [from] [to]
//! ```
//!
//! Prints one line per frame with the macroblock counters of the decoder, so
//! the first frame that introduces a tool (4MV, dquant, P-intra, TCOEFF
//! escapes) can be located exactly.

use wc3_bink::mpeg4::Mpeg4Decoder;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: mb_stats <movie> [from] [to]");
        std::process::exit(2);
    }
    let path = &args[0];
    let from: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let to: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let file = wc3_bink::BinkFile::open(path).expect("open movie");
    let mut dec = Mpeg4Decoder::new();
    let avi = file.avi();
    let mut prev = [0u32; 6];
    for i in 0..file.frame_count() {
        let payload = avi.video_frame(i).expect("frame payload");
        if let Err(err) = dec.decode(payload) {
            println!("frame {i}: DECODE ERROR {err}");
            break;
        }
        if i < from {
            continue;
        }
        if i > to {
            break;
        }
        let s = dec.stats();
        let cur = [s.p_intra, s.four_mv, s.dquant, s.escapes1, s.escapes2, s.escapes3];
        let changed = i == 0 || cur.iter().zip(prev.iter()).any(|(a, b)| a > b);
        prev = cur;
        if changed {
            let vop = dec.last_vop().cloned();
            let (vtype, vq) = vop.as_ref().map_or(("?", 0), |v| {
                (match v.coding_type {
                    wc3_bink::PictureType::I => "I",
                    wc3_bink::PictureType::P => "P",
                    wc3_bink::PictureType::B => "B",
                    _ => "?",
                }, v.qscale)
            });
            println!(
                "frame {i:5} type={} q={} p_intra={} four_mv={} dquant={} esc={}/{}/{}",
                vtype,
                vq,
                s.p_intra,
                s.four_mv,
                s.dquant,
                s.escapes1,
                s.escapes2,
                s.escapes3
            );
        }
    }
}
