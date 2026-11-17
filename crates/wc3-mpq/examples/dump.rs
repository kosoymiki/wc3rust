//! Bounded MPQ inspector: `dump <archive> <name> [out-file]`.
use std::env;
use wc3_mpq::MpqArchive;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump <archive> <name> [out-file]");
        return;
    }
    let archive = MpqArchive::open(&args[1]).expect("open archive");
    let bytes = archive.read(&args[2]).expect("read file");
    match args.get(3) {
        Some(path) => {
            std::fs::write(path, &bytes).expect("write");
            println!("wrote {path} len={}", bytes.len());
        }
        None => {
            let limit = 900.min(bytes.len());
            println!("len={} head={:?}", bytes.len(), &bytes[..limit]);
            println!("text:\n{}", String::from_utf8_lossy(&bytes[..limit]));
        }
    }
}
