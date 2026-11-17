//! Print MPQ block metadata for one or more names.
//!
//! `cargo run -p wc3-mpq --example mpq_meta -- [--mpq PATH] NAME [NAME...]`

use std::path::{Path, PathBuf};

use wc3_mpq::{CodecUsage, MpqArchive};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = PathBuf::from(
        std::env::var_os("WC3_MPQ")
            .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()),
    );
    let mut names: Vec<String> = Vec::new();
    let mut dump: Option<(String, String)> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--mpq" {
            i += 1;
            mpq = PathBuf::from(&args[i]);
        } else if args[i] == "--extract" {
            i += 1;
            let name = args[i].clone();
            i += 1;
            dump = Some((name, args[i].clone()));
        } else {
            names.push(args[i].clone());
        }
        i += 1;
    }
    if names.is_empty() && dump.is_none() {
        return Err("usage: mpq_meta [--mpq PATH] [--extract NAME OUT] NAME...".into());
    }
    let archive = MpqArchive::open(&mpq)?;
    println!(
        "archive={} size={} blocks={} listfile={} sector_size={}",
        mpq.display(),
        archive.fingerprint().archive_size,
        archive.block_count(),
        archive.listfile().len(),
        archive.sector_size()
    );
    if let Some((name, out)) = &dump {
        let bytes = archive.read(name)?;
        if let Some(parent) = Path::new(out).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, &bytes)?;
        println!("extracted {name} -> {out} ({} bytes)", bytes.len());
    }
    for name in &names {
        match archive.meta(name) {
            Some((block_index, meta)) => {
                println!(
                    "{name}\n  block={block_index} offset={} csize={} size={} flags=0x{:08x} mask=0x{:02x} single_unit={} encrypted={} compressed={} declared={}",
                    meta.block_index,
                    meta.compressed_size,
                    meta.size,
                    meta.flags,
                    meta.mask,
                    meta.is_single_unit(),
                    meta.is_encrypted(),
                    meta.is_compressed(),
                    meta.declared_usage().describe()
                );
                match archive.read_with_info(name) {
                    Ok((bytes, usage)) => println!(
                        "  read ok: {} bytes, observed codecs={} ({:02x})",
                        bytes.len(),
                        usage.describe(),
                        usage.0
                    ),
                    Err(err) => println!("  read FAILED: {err}"),
                }
                let _ = CodecUsage::STORED;
            }
            None => println!("{name}\n  not in archive"),
        }
    }
    Ok(())
}
