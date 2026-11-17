//! Geoset/bone diagnosis for the "doubled bodies, detached plates" artifact.
//!
//! ```text
//! cargo run -p wc3-formats --example mdx_geoset_probe [model path ...]
//! ```
//!
//! Prints, per geoset: LOD, material, selection group, vertex/face counts,
//! bounds and the matrix groups it uses; then the bone array and the maximum
//! `MATS` index, so a skinning or draw-loop bug can be told apart from a data
//! duplicate.

use std::path::Path;

use wc3_formats::mdx::MdxModel;
use wc3_mpq::MpqArchive;

fn main() {
    let mpq = std::env::args().nth(1).unwrap_or_else(|| {
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".to_string()
    });
    let mut paths: Vec<String> = std::env::args().skip(2).collect();
    if paths.is_empty() {
        paths = vec![
            "units\\orc\\Peon\\Peon.mdx".to_string(),
            "units\\human\\Footman\\Footman.mdx".to_string(),
        ];
    }
    let archive = MpqArchive::open(Path::new(&mpq)).expect("open archive");
    for path in paths {
        let Ok(bytes) = archive.read(&path) else {
            println!("{path}: read failed");
            continue;
        };
        let Ok(model) = MdxModel::parse(&bytes) else {
            println!("{path}: parse failed");
            continue;
        };
        println!(
            "\n{path}: version={} geosets={} materials={} bones={} helpers={} pivots={} geoset_anims={}",
            model.version,
            model.geosets.len(),
            model.materials.len(),
            model.bones.len(),
            model.helpers.len(),
            model.pivots.len(),
            model.geoset_animations.len(),
        );
        let mut matrix_max = 0u32;
        for (index, geoset) in model.geosets.iter().enumerate() {
            matrix_max = matrix_max.max(geoset.matrix_groups.iter().copied().max().unwrap_or(0));
            println!(
                "  geoset[{index}] lod={} mat={} sel={} verts={} tris={} uv_sets={} gndx_max={} groups={:?} bounds=({:.1},{:.1},{:.1})..({:.1},{:.1},{:.1})",
                geoset.lod,
                geoset.material_id,
                geoset.selection_group,
                geoset.vertices.len(),
                geoset.faces.len() / 3,
                geoset.uv_sets.len(),
                geoset.vertex_groups.iter().copied().max().unwrap_or(0),
                geoset.matrix_groups,
                geoset.bounds.min.x,
                geoset.bounds.min.y,
                geoset.bounds.min.z,
                geoset.bounds.max.x,
                geoset.bounds.max.y,
                geoset.bounds.max.z,
            );
        }
        println!(
            "  materials: {:?}",
            model
                .materials
                .iter()
                .enumerate()
                .map(|(index, material)| (index, material.layers.len()))
                .collect::<Vec<_>>()
        );
        println!(
            "  bones (chunk order): object_id -> {:?}",
            model
                .bones
                .iter()
                .map(|bone| (bone.node.object_id, bone.node.name.clone()))
                .collect::<Vec<_>>()
        );
        println!("  max MATS index = {matrix_max} (bones-1 = {})", model.bones.len().saturating_sub(1));
        println!(
            "  geoset_animations: {:?}",
            model
                .geoset_animations
                .iter()
                .map(|animation| (animation.geoset_id, animation.alpha, animation.alpha_track.is_some(), animation.color_track.is_some()))
                .collect::<Vec<_>>()
        );
        // Duplicate-mesh signature: two geosets with the same vertex count and
        // the same bounds would be a duplicated body in the file itself.
        for i in 0..model.geosets.len() {
            for j in (i + 1)..model.geosets.len() {
                let a = &model.geosets[i];
                let b = &model.geosets[j];
                let same_bounds = a.bounds.min == b.bounds.min && a.bounds.max == b.bounds.max;
                if a.vertices.len() == b.vertices.len() && same_bounds {
                    println!("  DUPLICATE SIGNATURE: geoset {i} and {j} share vertex count and bounds");
                }
            }
        }
    }
}
