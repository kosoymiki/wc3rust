//! Classify block flags across the archive: which files are encrypted,
//! compressed, single-unit, and which encrypted files are larger than one
//! sector (the per-sector decryption case).
//!
//! `cargo run -p wc3-mpq --example block_stats -- [--mpq PATH]`

use std::collections::BTreeMap;
use std::path::PathBuf;

use wc3_mpq::MpqArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = PathBuf::from(
        std::env::var_os("WC3_MPQ")
            .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()),
    );
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--mpq" {
            i += 1;
            mpq = PathBuf::from(&args[i]);
        }
        i += 1;
    }
    let archive = MpqArchive::open(&mpq)?;
    let sector = archive.sector_size() as u64;
    println!(
        "archive={} blocks={} listfile={} sector_size={sector}",
        mpq.display(),
        archive.block_count(),
        archive.listfile().len()
    );
    let mut classes: BTreeMap<&'static str, (usize, u64)> = BTreeMap::new();
    let mut multisector_encrypted = Vec::new();
    let mut total = 0usize;
    let mut bytes = 0u64;
    for name in archive.listfile() {
        let Some((_, meta)) = archive.meta(name) else {
            continue;
        };
        if meta.flags & 0x8000_0000 == 0 {
            continue;
        }
        total += 1;
        bytes += meta.size as u64;
        let class = match (
            meta.is_encrypted(),
            meta.is_compressed(),
            meta.is_single_unit(),
        ) {
            (false, false, _) => "plain-stored",
            (false, true, true) => "plain-compressed-single",
            (false, true, false) => "plain-compressed-sector",
            (true, false, true) => "encrypted-stored-single",
            (true, false, false) => "encrypted-stored-sector",
            (true, true, true) => "encrypted-compressed-single",
            (true, true, false) => "encrypted-compressed-sector",
        };
        let slot = classes.entry(class).or_default();
        slot.0 += 1;
        slot.1 += meta.size as u64;
        if meta.is_encrypted()
            && !meta.is_compressed()
            && (meta.size as u64) > sector
        {
            multisector_encrypted.push((name.clone(), meta.size, meta.compressed_size));
        }
    }
    for (class, (count, size)) in &classes {
        println!("{class}: files={count} bytes={size}");
    }
    println!(
        "total={total} bytes={bytes} encrypted_uncompressed_multisector={}",
        multisector_encrypted.len()
    );
    for (name, size, csize) in multisector_encrypted.iter().take(20) {
        println!("  {name} size={size} csize={csize}");
    }
    Ok(())
}
