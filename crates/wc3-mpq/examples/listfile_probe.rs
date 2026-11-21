//! Print (listfile) size and the first names resolved from it.
use std::env;
use wc3_mpq::MpqArchive;

fn main() {
    let path = env::args().nth(1).expect("mpq path");
    let archive = MpqArchive::open(&path).expect("open");
    let names = archive.listfile();
    println!("LISTFILE names={} entries={}", names.len(), names.len());
    for n in names.iter().take(5) { println!("  {n}"); }
}
