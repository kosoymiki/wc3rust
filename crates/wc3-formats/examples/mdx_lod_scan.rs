//! Scan every `.mdx` in an archive: MDX version, geoset count and LOD groups.
//!
//! ```text
//! cargo run -p wc3-formats --example mdx_lod_scan [War3.mpq]
//! ```
//!
//! The question it answers: does this install actually ship geoset LOD data
//! (`version > 800`, the only version that has the `lod` field), and if so, how
//! many models carry more than one LOD group.

use std::collections::BTreeMap;
use std::path::Path;

use wc3_formats::mdx::MdxModel;
use wc3_mpq::MpqArchive;

fn main() {
    let mpq = std::env::args().nth(1).unwrap_or_else(|| {
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".to_string()
    });
    let archive = MpqArchive::open(Path::new(&mpq)).expect("open archive");
    let models: Vec<&String> = archive
        .listfile()
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".mdx"))
        .collect();
    println!("archive {mpq}: {} listfile entries, {} .mdx", archive.listfile().len(), models.len());

    let mut versions: BTreeMap<u32, usize> = BTreeMap::new();
    let mut read_failed = 0usize;
    let mut parse_failed = 0usize;
    let mut with_lod_field = 0usize;
    let mut multi_group: Vec<(String, Vec<(u32, usize)>)> = Vec::new();
    let mut single_group_versions: BTreeMap<u32, usize> = BTreeMap::new();
    let mut geoset_total = 0usize;

    for path in &models {
        let bytes = match archive.read(path) {
            Ok(bytes) => bytes,
            Err(_) => {
                read_failed += 1;
                continue;
            }
        };
        // Header: `MDLX`, then chunks — `VERS` (tag+size+u32) is always first, so
        // the version can be classified without a full parse.
        if bytes.len() < 16 || &bytes[..4] != b"MDLX" || &bytes[4..8] != b"VERS" {
            parse_failed += 1;
            continue;
        }
        let version = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes"));
        *versions.entry(version).or_insert(0) += 1;
        if version <= 800 {
            *single_group_versions.entry(version).or_insert(0) += 1;
            continue;
        }
        with_lod_field += 1;
        let model = match MdxModel::parse(&bytes) {
            Ok(model) => model,
            Err(_) => {
                parse_failed += 1;
                continue;
            }
        };
        geoset_total += model.geosets.len();
        let groups = lod_groups(&model);
        if groups.len() > 1 {
            multi_group.push(((*path).clone(), groups));
        }
    }

    println!("versions = {versions:?}");
    println!("models without the lod field (version <= 800) = {single_group_versions:?}");
    println!("models with the lod field (version > 800) = {with_lod_field}, geosets = {geoset_total}");
    println!("models with more than one LOD group = {}", multi_group.len());
    for (path, groups) in multi_group.iter().take(20) {
        println!("  {path}: {groups:?}");
    }
    // A handful of representative units, for the render layer.
    for path in [
        "units\\orc\\Peon\\Peon.mdx",
        "units\\human\\Footman\\Footman.mdx",
        "units\\nightelf\\Archer\\Archer.mdx",
    ] {
        match archive.read(path).map(|bytes| MdxModel::parse(&bytes)) {
            Ok(Ok(model)) => println!(
                "{path}: version={} geosets={} lod_groups={:?}",
                model.version,
                model.geosets.len(),
                lod_groups(&model)
            ),
            Ok(Err(error)) => println!("{path}: parse failed: {error}"),
            Err(error) => println!("{path}: read failed: {error}"),
        }
    }
    println!("read_failed={read_failed} parse_failed={parse_failed}");
}

fn lod_groups(model: &MdxModel) -> Vec<(u32, usize)> {
    let mut out: Vec<(u32, usize)> = Vec::new();
    for geoset in &model.geosets {
        match out.iter_mut().find(|(lod, _)| *lod == geoset.lod) {
            Some((_, count)) => *count += 1,
            None => out.push((geoset.lod, 1)),
        }
    }
    out.sort();
    out
}
