//! Dump one MPQ block by index: block header, first bytes, and the decoded
//! payload (optionally to a file).
//!
//! The read path for listfile-less and pseudo-named entries; useful to compare
//! a block against a StormLib extraction.
//!
//! Usage: `cargo run -p wc3-mpq --example dbg_pk -- <archive> <block_index> [out]`
use std::os::unix::fs::FileExt;

use wc3_mpq::MpqArchive;

fn main() {
    let p = std::env::args().nth(1).expect("archive");
    let index: u32 = std::env::args()
        .nth(2)
        .expect("block index")
        .parse()
        .expect("index");
    let a = MpqArchive::open(&p).expect("open");
    let (offset, csize, size, flags) = a.block_entry(index).expect("block");
    println!("offset={offset} csize={csize} size={size} flags=0x{flags:08x}");
    let f = std::fs::File::open(&p).expect("file");
    let mut head = vec![0u8; 64.min(csize as usize)];
    f.read_exact_at(&mut head, offset as u64).expect("read");
    println!("head={:02x?}", head);
    match a.read_block(index) {
        Ok(b) => {
            println!("read ok len={} head={:02x?}", b.len(), &b[..8.min(b.len())]);
            if let Some(out) = std::env::args().nth(3) {
                std::fs::write(&out, &b).expect("write");
                println!("wrote {out}");
            }
        }
        Err(e) => println!("read err {e}"),
    }
}
