//! Reads one file out of an MPQ archive and greps it, for data-source checks.
//!
//!   cargo run -p wc3-sim --example mpq_probe -- <archive.mpq> <internal name> <needle>

use std::path::Path;

use wc3_mpq::MpqArchive;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: mpq_probe <archive.mpq> <internal name> <needle>");
        return;
    }
    let path = Path::new(&args[1]);
    let name = &args[2];
    let needle = args[3].to_lowercase();
    let base = match wc3_formats::map::map_mpq_base(path) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("base: {error}");
            return;
        }
    };
    let archive = match MpqArchive::open_at(path, base) {
        Ok(archive) => archive,
        Err(error) => {
            eprintln!("open: {error}");
            return;
        }
    };
    let bytes = match archive.read(name) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("read {name}: {error}");
            return;
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut hits = 0;
    for (index, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(&needle) {
            println!("{}:{}", index + 1, line);
            hits += 1;
            if hits >= 20 {
                break;
            }
        }
    }
    println!("bytes={} lines={} hits={hits}", bytes.len(), text.lines().count());
}
