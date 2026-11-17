//! Dump the geoset LOD groups of a model inside `War3.mpq`.
//!
//! ```text
//! cargo run -p wc3-formats --example mdx_lod -- units\orc\Peon\Peon.mdx [...]
//! ```

use std::path::Path;

use wc3_formats::mdx::{Geoset, MdxModel};
use wc3_mpq::MpqArchive;

fn main() {
    let mpq = std::env::args()
        .nth(1)
        .unwrap_or_else(|| {
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".to_string()
        });
    let mut paths: Vec<String> = std::env::args().skip(2).collect();
    if paths.is_empty() {
        paths = vec![
            "units\\orc\\Peon\\Peon.mdx".to_string(),
            "units\\human\\Footman\\Footman.mdx".to_string(),
            "units\\undead\\Acolyte\\Acolyte.mdx".to_string(),
        ];
    }
    let archive = MpqArchive::open(Path::new(&mpq)).expect("open War3.mpq");
    for path in paths {
        let bytes = match archive.read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("{path}: read failed: {error}");
                continue;
            }
        };
        let model = match MdxModel::parse(&bytes) {
            Ok(model) => model,
            Err(error) => {
                println!("{path}: parse failed: {error}");
                continue;
            }
        };
        println!(
            "{path}: version={} geosets={} materials={} sequences={}",
            model.version,
            model.geosets.len(),
            model.materials.len(),
            model.sequences.len()
        );
        for (index, geoset) in model.geosets.iter().enumerate() {
            println!(
                "  geoset[{index}] lod={} lod_name={:?} material={} selection={} flags={} verts={} faces={}",
                geoset.lod,
                geoset.lod_name,
                geoset.material_id,
                geoset.selection_group,
                geoset.selection_flags,
                geoset.vertices.len(),
                geoset.faces.len() / 3,
            );
        }
        println!("  lod groups = {:?}", lod_groups(&model.geosets));
        println!(
            "  lowest lod = {:?} (geosets at that level: {})",
            Geoset::lowest_lod(&model.geosets),
            model
                .geosets
                .iter()
                .filter(|geoset| Some(geoset.lod) == Geoset::lowest_lod(&model.geosets))
                .count()
        );
    }
}

fn lod_groups(geosets: &[Geoset]) -> Vec<(u32, usize)> {
    let mut out: Vec<(u32, usize)> = Vec::new();
    for geoset in geosets {
        match out.iter_mut().find(|(lod, _)| *lod == geoset.lod) {
            Some((_, count)) => *count += 1,
            None => out.push((geoset.lod, 1)),
        }
    }
    out.sort();
    out
}
