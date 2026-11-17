//! Ledger 451 — `render.skinning.influences-survive-compaction`.
//!
//! `Assets::load` compacts every mesh before any reader sees it, so the compact
//! record is the only skin data the runtime has. These tests re-derive the
//! authored influence lists from a fresh parse of the same MDX and pin what
//! `MeshData::compact` (and therefore `CompactMesh::bones`, the single accessor
//! the GPU upload uses) keeps: the four heaviest pairs with their bone indices,
//! weights that renormalise to 1, and no weighted source vertex losing its
//! skinning.

use std::path::PathBuf;

use glam::{Vec2, Vec3};
use wc3_anim::GeosetSkin;
use wc3_assetdb::AssetDb;
use wc3_formats::mdx::Geoset;
use wc3_game::assets::{BoneWeight, MeshData};

/// Def id + the MDX the shipped install resolves for it (the exact files
/// `Assets::load` reads; War3x does not override any of them).
const UNITS: [(&str, &str); 4] = [
    ("hfoo", "Units\\Human\\Footman\\Footman.mdx"),
    ("harc", "Units\\NightElf\\Archer\\Archer.mdx"),
    ("ogru", "Units\\Orc\\Grunt\\Grunt.mdx"),
    ("opeo", "Units\\Orc\\Peon\\Peon.mdx"),
];

fn mpq_path() -> Option<PathBuf> {
    [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

/// The runtime mesh `build_mesh` would produce for this geoset, compacted.
///
/// Only the fields the compact format keeps are populated: positions, normals,
/// UVs, indices and the canonical influence lists.
fn compacted_from_source(geoset: &Geoset) -> MeshData {
    let skin = GeosetSkin::from_geoset(geoset);
    let count = geoset.vertices.len();
    let mut mesh = MeshData::default();
    mesh.positions = geoset.vertices.clone();
    mesh.normals = if geoset.normals.len() == count {
        geoset.normals.clone()
    } else {
        vec![Vec3::Y; count]
    };
    mesh.uvs = geoset
        .uv_sets
        .first()
        .filter(|set| set.len() == count)
        .cloned()
        .unwrap_or_else(|| vec![Vec2::ZERO; count]);
    mesh.indices = geoset.faces.iter().map(|&index| index as u32).collect();
    mesh.bones = skin
        .influences
        .iter()
        .map(|list| {
            list.iter()
                .map(|&(bone, weight)| BoneWeight { bone, weight })
                .collect()
        })
        .collect();
    let before = mesh.vertex_record_bytes();
    mesh.compact();
    assert!(
        mesh.compact.is_some(),
        "a geoset with {} vertices must compact",
        count
    );
    assert!(
        before > mesh.vertex_record_bytes(),
        "the compact record is smaller than the f32 record ({before} -> {})",
        mesh.vertex_record_bytes()
    );
    mesh
}

/// The contract the compact format promises: the four heaviest influences with
/// the surviving weights renormalised to 1.
fn expected_top_four(list: &[(u16, f32)]) -> Vec<(u16, f32)> {
    let mut kept: Vec<(u16, f32)> = list.to_vec();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1));
    kept.truncate(4);
    let total: f32 = kept.iter().map(|pair| pair.1).sum();
    if total > 0.0 {
        for pair in &mut kept {
            pair.1 /= total;
        }
    }
    kept
}

#[test]
fn influences_survive_compaction() {
    let Some(mpq) = mpq_path() else {
        eprintln!("SKIP render.skinning.influences-survive-compaction: War3.mpq missing");
        return;
    };
    let mut db = AssetDb::open_mpq(&mpq).expect("open War3.mpq");
    for (def, path) in UNITS {
        let model = db.model(path).unwrap_or_else(|error| panic!("{def}: {path}: {error:?}"));
        let mut source_weighted = 0usize;
        let mut compact_weighted = 0usize;
        let mut source_max_bone = 0u16;
        let mut compact_max_bone = 0u16;
        let mut max_sum_error = 0.0f32;
        let mut max_influences = 0usize;
        let mut vertices = 0usize;
        let mut record_bytes = 0usize;
        let mut expanded_bytes = 0usize;
        for (geoset_id, geoset) in model.geosets.iter().enumerate() {
            let skin = GeosetSkin::from_geoset(geoset);
            let mesh = compacted_from_source(geoset);
            let compact = mesh.compact.as_ref().expect("compacted mesh");
            assert_eq!(
                compact.vertex_count as usize,
                geoset.vertices.len(),
                "{def} geoset {geoset_id}: compaction keeps every vertex"
            );
            vertices += compact.vertex_count as usize;
            record_bytes = mesh.vertex_record_bytes();
            expanded_bytes += mesh.vertex_count() * 32; // f32 pos+normal+uv record
            for vertex in 0..compact.vertex_count as usize {
                let source = skin
                    .influences
                    .get(vertex)
                    .cloned()
                    .unwrap_or_default();
                if !source.is_empty() {
                    source_weighted += 1;
                    for &(bone, _) in &source {
                        source_max_bone = source_max_bone.max(bone);
                    }
                }
                let list = compact.bones(vertex);
                assert!(
                    list.len() <= 4,
                    "{def} geoset {geoset_id} vertex {vertex}: {} influences > 4",
                    list.len()
                );
                if source.is_empty() {
                    assert!(
                        list.is_empty(),
                        "{def} geoset {geoset_id} vertex {vertex}: unweighted source gained {list:?}"
                    );
                    continue;
                }
                // Ledger 451 criterion: every weighted source vertex keeps at
                // least one influence after compaction.
                assert!(
                    !list.is_empty(),
                    "{def} geoset {geoset_id} vertex {vertex}: lost all {} source influences",
                    source.len()
                );
                compact_weighted += 1;
                max_influences = max_influences.max(list.len());
                for pair in &list {
                    compact_max_bone = compact_max_bone.max(pair.bone);
                }
                let total: f32 = list.iter().map(|pair| pair.weight).sum();
                max_sum_error = max_sum_error.max((total - 1.0).abs());
                assert!(
                    (total - 1.0).abs() <= 1e-3,
                    "{def} geoset {geoset_id} vertex {vertex}: weights sum to {total}"
                );
                let expected = expected_top_four(&source);
                assert_eq!(
                    list.len(),
                    expected.len(),
                    "{def} geoset {geoset_id} vertex {vertex}: kept {} of {} authored pairs",
                    list.len(),
                    expected.len()
                );
                for (actual, (bone, weight)) in list.iter().zip(&expected) {
                    assert_eq!(
                        actual.bone, *bone,
                        "{def} geoset {geoset_id} vertex {vertex}: bone index moved"
                    );
                    assert!(
                        (actual.weight - weight).abs() <= 5e-3,
                        "{def} geoset {geoset_id} vertex {vertex}: weight {} vs authored {weight}",
                        actual.weight
                    );
                }
            }
        }
        println!(
            "SKIN def={def} verts={vertices} source_weighted={source_weighted} \
             compact_weighted={compact_weighted} max_influences={max_influences} \
             source_max_bone={source_max_bone} compact_max_bone={compact_max_bone} \
             max_sum_err={max_sum_error:.7} record_bytes={record_bytes} \
             f32_record_bytes={} f32_bytes={expanded_bytes}",
            expanded_bytes / vertices.max(1)
        );
        assert_eq!(
            compact_weighted, source_weighted,
            "{def}: every weighted source vertex survives compaction"
        );
        assert_eq!(
            compact_max_bone, source_max_bone,
            "{def}: compaction keeps the authored bone index range"
        );
        assert_eq!(record_bytes, 20, "{def}: 20 B compact vertex record");
        assert!(source_weighted > 0, "{def}: the fixture has skinned vertices");
    }
}
