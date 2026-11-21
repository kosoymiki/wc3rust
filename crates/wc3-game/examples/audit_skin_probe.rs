//! WC3-I-014 / I-015 / I-016 probe: skin and skeleton invariants.
//!
//! 1. **I-016** — a one-node model whose `object_id` is near `u32::MAX` used to
//!    size a dense `Vec<Option<usize>>` by `max_id + 1` (≈34 GB). The bounded
//!    `NodeIndex` (dense prefix + sorted sparse tail) maps the id with a constant
//!    amount of memory, and parent links through sparse ids still resolve.
//! 2. **I-014** — an influence whose bone index is past the model's palette is
//!    dropped and the remaining weights renormalised before any upload, so the
//!    GPU's `palette[base + bone]` cannot read outside the unit's palette.
//! 3. **I-015** — a classic matrix group with six matrices is truncated to the
//!    five-influence support profile, renormalised to sum 1.0, and *counted*
//!    (`truncated_vertices`) so the load path can log it.
//!
//! Run: `scripts/h2-cargo run --release -p wc3-game --example audit_skin_probe`.

use glam::{Quat, Vec2, Vec3};
use wc3_formats::mdx::{Bone, Extent, Geoset, Interpolation, MdxModel, Node, Track};
use wc3_sim::UnitKind;

// Fits `i32` because MDX parents are stored as i32 (`Node::parent_id`); still
// large enough that the old `max_id + 1` dense table would have requested ~17 GB.
const SPARSE_ID: u32 = 0x7FFF_FFF0;

fn track<T: Copy>(value: T) -> Track<T> {
    Track {
        interpolation: Interpolation::None,
        global_sequence: None,
        keys: Vec::new(),
        static_value: Some(value),
    }
}

fn node(name: &str, object_id: u32, parent_id: i32) -> Node {
    Node {
        name: name.to_string(),
        object_id,
        parent_id,
        flags: 0,
        translation: track(Vec3::ZERO),
        rotation: track(Quat::IDENTITY),
        scaling: track(Vec3::ONE),
    }
}

fn bone(name: &str, object_id: u32, parent_id: i32) -> Bone {
    Bone {
        node: node(name, object_id, parent_id),
        geoset_id: -1,
        geoset_animation_id: -1,
    }
}

fn empty_geoset() -> Geoset {
    Geoset {
        vertices: Vec::new(),
        normals: Vec::new(),
        face_type_groups: Vec::new(),
        face_groups: Vec::new(),
        faces: Vec::new(),
        vertex_groups: Vec::new(),
        matrix_groups: Vec::new(),
        matrix_indices: Vec::new(),
        material_id: 0,
        selection_group: 0,
        selection_flags: 0,
        lod: 0,
        lod_name: String::new(),
        bounds: Extent {
            radius: 1.0,
            min: Vec3::ZERO,
            max: Vec3::ONE,
        },
        sequence_bounds: Vec::new(),
        uv_sets: Vec::new(),
        skin: Vec::new(),
    }
}

fn sparse_skeleton() {
    println!("== I-016: skeleton allocation vs max object id ==");
    // One bone at id 0 parented to a helper at u32::MAX-1: the dense table the
    // old code built had `max_id + 1` slots.
    let mut model = MdxModel::default();
    model.bones = vec![bone("body", 0, SPARSE_ID as i32)]; // SPARSE_ID < i32::MAX
    model.helpers = vec![node("helper", SPARSE_ID, -1)];
    model.pivots = vec![Vec3::ZERO; 2];
    let old_slots = SPARSE_ID as u64 + 1;
    let old_bytes = old_slots * std::mem::size_of::<Option<usize>>() as u64;
    let skeleton = wc3_anim::Skeleton::from_model(&model);
    let index = &skeleton.index_by_object_id;
    let new_bytes =
        index.dense_len() as u64 * std::mem::size_of::<Option<usize>>() as u64
            + index.sparse_len() as u64 * (std::mem::size_of::<u32>() + std::mem::size_of::<usize>()) as u64;
    println!(
        "nodes={} sparse_id={SPARSE_ID:#x} old_dense_slots={old_slots} old_bytes={old_bytes} \
         new_dense_slots={} new_sparse_entries={} new_bytes={new_bytes}",
        skeleton.nodes.len(),
        index.dense_len(),
        index.sparse_len(),
    );
    println!(
        "lookup_body={:?} lookup_helper={:?} parent_of_body={:?} (sparse parent resolves)",
        index.get(0),
        index.get(SPARSE_ID),
        skeleton.parents.first(),
    );
    // The runtime pose path (`Animator`) indexes its node array by object id;
    // it must allocate the capped slot count, not `max_id + 1`.
    let animator = wc3_anim::Animator::from_model(&model);
    let worlds = animator.world_matrices();
    println!(
        "animator_slots={} cap={} world_matrices={}",
        wc3_anim::animator_slot_count(&model),
        wc3_anim::MAX_ANIMATOR_SLOTS,
        worlds.len(),
    );
    assert!(wc3_anim::animator_slot_count(&model) <= wc3_anim::MAX_ANIMATOR_SLOTS);
    assert_eq!(worlds.len(), wc3_anim::animator_slot_count(&model));
    assert_eq!(index.get(SPARSE_ID), Some(1));
    assert_eq!(skeleton.parents.first().copied().flatten(), Some(1));
    assert!(new_bytes < 1 << 20, "bounded index, got {new_bytes} bytes");
}

fn dense_skeleton() {
    let mut model = MdxModel::default();
    model.bones = (0..40).map(|id| bone("b", id, id as i32 - 1)).collect();
    model.pivots = vec![Vec3::ZERO; 40];
    let skeleton = wc3_anim::Skeleton::from_model(&model);
    let index = &skeleton.index_by_object_id;
    let parents_ok = skeleton
        .parents
        .iter()
        .enumerate()
        .all(|(index, parent)| match (index, parent) {
            (0, None) => true,
            (_, Some(parent)) => *parent == index - 1,
            _ => false,
        });
    println!(
        "dense model nodes={} dense_slots={} sparse_entries={} parents_ok={parents_ok}",
        skeleton.nodes.len(),
        index.dense_len(),
        index.sparse_len(),
    );
    assert!(parents_ok);
}

fn bone_range() {
    println!("== I-014: out-of-palette bone index dropped before upload ==");
    let mut geoset = empty_geoset();
    geoset.vertices = vec![Vec3::ZERO; 4];
    geoset.vertex_groups = vec![0, 0, 1, 1];
    geoset.matrix_groups = vec![2, 1];
    // Group 0 mixes a real bone (0) with id 999, which no 2-bone palette has.
    geoset.matrix_indices = vec![0, 999, 1];
    let mut skin = wc3_anim::GeosetSkin::from_geoset(&geoset);
    let before: Vec<u16> = skin
        .influences
        .iter()
        .flat_map(|list| list.iter().map(|(bone, _)| *bone))
        .collect();
    let dropped = skin.drop_out_of_range_bones(2);
    let after: Vec<u16> = skin
        .influences
        .iter()
        .flat_map(|list| list.iter().map(|(bone, _)| *bone))
        .collect();
    let sums: Vec<f32> = skin
        .influences
        .iter()
        .map(|list| list.iter().map(|(_, weight)| *weight).sum())
        .collect();
    println!("bones_before={before:?} bones_after={after:?}");
    println!(
        "dropped={dropped} in_range={} weight_sums={:?}",
        after.iter().all(|bone| (*bone as usize) < 2),
        sums
    );
    assert_eq!(dropped, 2, "one bad influence on each of the two group-0 vertices");
    assert!(after.iter().all(|bone| (*bone as usize) < 2));
    assert!(sums.iter().all(|sum| (sum - 1.0).abs() < 1e-6));
}

fn influences() {
    println!("== I-015: influence limit is explicit, normalised and counted ==");
    // Classic group of six matrices: the support profile keeps five.
    let mut geoset = empty_geoset();
    geoset.vertices = vec![Vec3::ZERO];
    geoset.vertex_groups = vec![0];
    geoset.matrix_groups = vec![6];
    geoset.matrix_indices = vec![0, 1, 2, 3, 4, 5];
    let skin = wc3_anim::GeosetSkin::from_geoset(&geoset);
    let list = &skin.influences[0];
    let sum: f32 = list.iter().map(|(_, weight)| *weight).sum();
    println!(
        "classic_level=6 limit={} kept={} truncated_vertices={} weight_sum={sum:.4}",
        wc3_anim::MAX_INFLUENCES,
        list.len(),
        skin.truncated_vertices,
    );
    assert_eq!(list.len(), wc3_anim::MAX_INFLUENCES);
    assert_eq!(skin.truncated_vertices, 1);
    assert!((sum - 1.0).abs() < 1e-6);

    // Packed SKIN carries exactly four pairs per vertex (8 u16), so it can
    // never exceed the five-influence profile; the >5 case is the classic
    // matrix-group path above. This pins that the packed path is untouched.

    // Four influences: under the profile, untouched and still normalised.
    let mut four = empty_geoset();
    four.vertices = vec![Vec3::ZERO];
    four.skin = vec![0, 60, 1, 60, 2, 60, 3, 60, 0, 0, 0, 0, 0, 0, 0, 0];
    let skin = wc3_anim::GeosetSkin::from_geoset(&four);
    let sum: f32 = skin.influences[0].iter().map(|(_, weight)| *weight).sum();
    println!(
        "packed_pairs=4 kept={} truncated_vertices={} weight_sum={sum:.4}",
        skin.influences[0].len(),
        skin.truncated_vertices,
    );
    assert_eq!(skin.influences[0].len(), 4);
    assert_eq!(skin.truncated_vertices, 0);
}

fn main() {
    // The probe only uses CPU-side data structures; the unit kind import pins
    // that this example builds against the same crate graph as the game.
    let _ = UnitKind::Footman;
    sparse_skeleton();
    dense_skeleton();
    bone_range();
    influences();
    println!("SKIN_PROBE PASS");
}
