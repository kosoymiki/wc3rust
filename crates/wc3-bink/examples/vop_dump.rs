//! Dump per-frame VOP headers: type, rounding, qscale, f_code, thresholds.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example vop_dump -- <movie> [from] [to]
//! ```

use wc3_bink::BinkFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: vop_dump <movie> [from] [to]");
        std::process::exit(2);
    }
    let path = &args[0];
    let from: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let to: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let mut file = BinkFile::open(path).expect("open movie");
    let total = file.frame_count();
    for i in 0..total {
        if file.decode_frame(i).is_err() {
            println!("frame {i}: DECODE ERROR");
            break;
        }
        if i < from {
            continue;
        }
        if i > to {
            break;
        }
        let v = file.last_vop().cloned();
        match v {
            Some(v) => println!(
                "frame {i:5} {:?} rounding={} q={} intra_dc_thr={} f_code={} b_code={} alt_scan={} time_incr={}",
                v.coding_type,
                v.rounding,
                v.qscale,
                v.intra_dc_threshold,
                v.f_code,
                v.b_code,
                v.alternate_scan,
                v.time_incr
            ),
            None => println!("frame {i}: no VOP"),
        }
    }
}
