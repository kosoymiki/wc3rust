//! Diagnostic (tech-lead): what geosets/meshes the drawn Peon actually has.
use std::path::Path;
use wc3_game::assets::{Assets, Carry, SequenceKind};

fn main() {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.herofolends.p0/files/War3.mpq",
    ]
    .iter()
    .map(Path::new)
    .find(|p| p.is_file());
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    let Some(mpq) = mpq else { println!("no install"); return };
    let assets = Assets::load(Some(mpq), Some(&map));
    for def in ["opeo", "hfoo", "ogre"] {
        let model = assets.model_for_def(def);
        println!(
            "def={def} model={:?} geosets={} meshes={}",
            model.model.as_ref().map(|m| m.name.clone()),
            model.model.as_ref().map(|m| m.geosets.len()).unwrap_or(0),
            model.meshes.len()
        );
        if let Some(mdx) = model.model.as_ref() {
            for (index, geoset) in mdx.geosets.iter().enumerate() {
                println!(
                    "  geoset #{index:<2} verts={:<5} faces={:<5} mat={} sel_group={} sel_flags=0x{:x}",
                    geoset.vertices.len(),
                    geoset.faces.len(),
                    geoset.material_id,
                    geoset.selection_group,
                    geoset.selection_flags
                );
            }
            println!(
                "  sequences: {:?}",
                mdx.sequences.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
            );
            println!("  textures: {:?}", mdx.textures.iter().map(|t| (t.path.clone(), t.replaceable_id)).collect::<Vec<_>>());
            for (mi, material) in mdx.materials.iter().enumerate() {
                println!(
                    "  material #{mi} priority={} flags=0x{:x} layers={:?}",
                    material.priority,
                    material.flags,
                    material
                        .layers
                        .iter()
                        .map(|l| (
                            l.texture_id,
                            l.blend_mode,
                            l.coord_id,
                            l.alpha,
                        ))
                        .collect::<Vec<_>>()
                );
            }
            println!(
                "  pivot={:.1} scale={:.3} bounds_y=({:.1}..{:.1})",
                model.pivot,
                model.scale,
                mdx.geosets.first().map(|g| g.bounds.min.y).unwrap_or(0.0),
                mdx.geosets.first().map(|g| g.bounds.max.y).unwrap_or(0.0)
            );
            println!(
                "  geoset_anims: {:?}",
                mdx.geoset_animations
                    .iter()
                    .map(|a| {
                        (
                            a.geoset_id,
                            a.alpha,
                            a.flags,
                            a.alpha_track.as_ref().map(|t| t.keys.len()),
                        )
                    })
                    .collect::<Vec<_>>()
            );
            if let Some(g) = mdx.geosets.first() {
                println!(
                    "  seq_extents: {:?}",
                    g.sequence_bounds
                        .iter()
                        .take(6)
                        .map(|e| (e.min.y, e.max.y))
                        .collect::<Vec<_>>()
                );
            }
            for (kind, carry) in [
                (SequenceKind::Walk, Carry::None),
                (SequenceKind::Walk, Carry::Gold),
                (SequenceKind::Walk, Carry::Lumber),
                (SequenceKind::Idle, Carry::Gold),
                (SequenceKind::Work, Carry::Gold),
                (SequenceKind::Work, Carry::Lumber),
                (SequenceKind::Attack, Carry::None),
            ] {
                let picked = model
                    .sequence_with(kind, carry)
                    .and_then(|i| mdx.sequences.get(i))
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "NONE".to_string());
                println!("  CARRY {kind:?}/{carry:?} -> {picked}");
            }
            // CARRY_VIS: which geosets are visible in each carry clip?
            for wanted in [
                "Walk",
                "Walk Gold",
                "Walk Lumber",
                "Stand Work",
                "stand work gold",
                "Stand Work Lumber",
            ] {
                let Some(seq) = mdx
                    .sequences
                    .iter()
                    .find(|candidate| candidate.name.eq_ignore_ascii_case(wanted))
                    .cloned()
                else {
                    println!("  CARRY_VIS {wanted:<18} MISSING");
                    continue;
                };
                let mid = seq.start.saturating_add(seq.end.saturating_sub(seq.start) / 2);
                let alphas = assets.mesh_alpha(
                    assets.model_index_for_def(def),
                    Some(&seq),
                    mid,
                );
                println!(
                    "  CARRY_VIS {wanted:<18} alphas={:?}",
                    alphas.iter().map(|a| (a * 100.0).round() / 100.0).collect::<Vec<_>>()
                );
            }
            let index = model.sequence(SequenceKind::Idle);
            if let Some(seq) = index.and_then(|i| mdx.sequences.get(i)) {
                let alphas = assets.mesh_alpha(
                    assets.model_index_for_def(def),
                    Some(seq),
                    0,
                );
                println!(
                    "  Stand seq {:?} alphas={:?}",
                    seq.name,
                    alphas.iter().map(|a| (a * 100.0).round() / 100.0).collect::<Vec<_>>()
                );
            }
        }
        for (i, mesh) in model.meshes.iter().enumerate() {
            println!(
                "  mesh #{i:<2} geoset={:<2} verts={:<5} tris={:<5} hide={} layers={}",
                mesh.geoset_id,
                mesh.positions.len(),
                mesh.indices.len() / 3,
                mesh.hide,
                mesh.layers.len()
            );
        }
    }
}
