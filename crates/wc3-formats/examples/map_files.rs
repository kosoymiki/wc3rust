//! Dump inner files of a `.w3m`/`.w3x` map container.
//!
//! `cargo run -p wc3-formats --example map_files -- <map> <outdir> [names...]`
//!
//! Without `names` the canonical inner-file list is tried (the editor names);
//! the map's own listfile is used to extend the set when present. RoC `.w3m`
//! files start with a 512-byte `HM3W` header, so the archive base comes from
//! [`wc3_formats::map::map_mpq_base`].

use std::path::PathBuf;

use wc3_formats::map::map_mpq_base;
use wc3_mpq::MpqArchive;

const INNER: &[&str] = &[
    "war3map.j",
    "scripts\\war3map.j",
    "war3map.w3e",
    "war3map.w3i",
    "war3map.w3u",
    "war3map.w3t",
    "war3map.w3a",
    "war3map.w3d",
    "war3map.w3b",
    "war3map.w3h",
    "war3map.w3q",
    "war3map.wts",
    "war3map.wtg",
    "war3map.wct",
    "war3map.w3r",
    "war3map.w3c",
    "war3map.w3s",
    "war3map.doo",
    "war3mapUnits.doo",
    "war3map.mmp",
    "war3map.imp",
    "war3mapMap.blp",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let map = PathBuf::from(args.next().expect("map path"));
    let outdir = PathBuf::from(args.next().expect("output directory"));
    let requested: Vec<String> = args.collect();
    std::fs::create_dir_all(&outdir)?;
    let base = map_mpq_base(&map)?;
    let archive = MpqArchive::open_at(&map, base)?;
    let names: Vec<String> = if requested.is_empty() {
        let mut names: Vec<String> = INNER.iter().map(|s| s.to_string()).collect();
        for listed in archive.listfile() {
            if !names.iter().any(|known| known.eq_ignore_ascii_case(listed)) {
                names.push(listed.clone());
            }
        }
        names
    } else {
        requested
    };
    let mut written = 0usize;
    for name in &names {
        match archive.read(name) {
            Ok(bytes) => {
                let target = outdir.join(name.replace(['\\', '/'], "/"));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, &bytes)?;
                println!("FILE {name} bytes={} out={}", bytes.len(), target.display());
                written += 1;
            }
            Err(wc3_mpq::MpqError::FileNotFound(_)) => {}
            Err(err) => println!("ERROR {name}: {err}"),
        }
    }
    println!("SUMMARY written={written} base={base}");
    Ok(())
}
