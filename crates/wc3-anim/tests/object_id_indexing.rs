//! Object-id node indexing verification for every unit/building model.
//!
//! The parity matrix requires one animator addressing nodes by MDX **object id**
//! (the game builds its palette that way) while skin weights and pivots come from
//! different chunks. This test walks the full corpus and proves the two
//! addressing modes coincide:
//!
//! 1. `PIVT.len() == MdxModel::node_object_count(bytes)` (every node object owns
//!    a pivot; cameras excluded),
//! 2. every bone's `object_id` equals its BONE-chunk index, so a bone-index
//!    palette and an object-id palette address the same matrices,
//! 3. every skin index is a valid bone and never points at a helper/attachment,
//! 4. the game's dense object-id palette (`Animator::world_matrices()` indexed by
//!    object id) equals the canonical bone-index skeleton pose, and skinning
//!    through it reproduces the canonical positions.
//!
//! Run: `cargo test -p wc3-anim --test object_id_indexing -- --ignored --nocapture`

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use wc3_anim::{Animator, GeosetSkin, ModelRuntime, Skeleton, skin_positions};
use wc3_formats::mdx::{AnimValue, MdxModel, Sequence};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn walk_mdx(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_mdx(&path, out);
        } else if path
            .extension()
            .map(|extension| extension.eq_ignore_ascii_case("mdx"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

fn pick(model: &MdxModel, needles: &[&str]) -> Option<usize> {
    model.sequences.iter().enumerate().find_map(|(index, sequence)| {
        (sequence.end > sequence.start
            && needles
                .iter()
                .any(|needle| sequence.name.to_ascii_lowercase().contains(needle)))
        .then_some(index)
    })
}

fn has_tracks(model: &MdxModel, sequence: &Sequence) -> bool {
    fn animated<T: AnimValue>(track: &wc3_formats::mdx::Track<T>, sequence: &Sequence) -> bool {
        track
            .keys
            .iter()
            .filter(|key| key.time >= sequence.start && key.time <= sequence.end)
            .count()
            >= 1
    }
    model
        .bones
        .iter()
        .map(|bone| &bone.node)
        .chain(model.helpers.iter())
        .any(|node| {
            animated(&node.translation, sequence)
                || animated(&node.rotation, sequence)
                || animated(&node.scaling, sequence)
        })
}

#[test]
#[ignore = "walks the full extracted MPQ tree (542 models); run explicitly"]
fn object_id_indexing_holds_for_every_model() {
    let root = repo_root();
    let mut files = Vec::new();
    for tree in ["analysis/war3_full/Units", "analysis/war3_full/Buildings"] {
        walk_mdx(&root.join(tree), &mut files);
    }
    assert!(!files.is_empty(), "no MDX fixtures");

    let mut csv = String::from(
        "model,bones,pivots,node_objects,bone_id_mismatches,max_skin_index,palette_delta,skin_delta,verdict\n",
    );
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut failures = Vec::new();
    let mut total_bones = 0usize;

    for file in &files {
        let Ok(bytes) = std::fs::read(file) else { continue };
        let Ok(model) = MdxModel::parse(&bytes) else {
            failures.push(format!("{}: parse", file.display()));
            fail += 1;
            continue;
        };
        let node_objects = MdxModel::node_object_count(&bytes).unwrap_or(0);
        let bone_count = model.bones.len();
        total_bones += bone_count;

        // 1. Pivot contract.
        let pivots_ok = node_objects == model.pivots.len();

        // 2. Every bone addressed by its own object id.
        let mut mismatches = 0usize;
        for (index, bone) in model.bones.iter().enumerate() {
            if bone.node.object_id as usize != index {
                mismatches += 1;
            }
        }

        // 3. Skin indices are bones, not helpers/attachments/events.
        let mut max_skin = 0usize;
        let mut skin_kind_bad = 0usize;
        for geoset in &model.geosets {
            for &bone in &geoset.matrix_indices {
                max_skin = max_skin.max(bone as usize);
                if bone as usize >= bone_count {
                    skin_kind_bad += 1;
                }
            }
        }

        // 4. Dense object-id palette (game) == canonical bone-index skeleton.
        let runtime = ModelRuntime::new(&model);
        let sequence_index = pick(&model, &["stand"])
            .or_else(|| pick(&model, &["portrait"]))
            .or_else(|| pick(&model, &["birth", "death"]))
            .unwrap_or(0);
        let sequence = &model.sequences[sequence_index];
        let time = sequence.start + (sequence.end - sequence.start) / 2;
        let mut animator = Animator::from_model(&model);
        animator.play(sequence);
        animator.time_ms = time as f32;
        let palette = animator.world_matrices();
        let canonical = skeleton_pose(&model, sequence, time, has_tracks(&model, sequence));
        let mut palette_delta = 0.0f32;
        for bone in 0..bone_count {
            if bone >= palette.len() || bone >= canonical.len() {
                palette_delta = f32::INFINITY;
                break;
            }
            let a = palette[bone].to_cols_array();
            let b = canonical[bone].to_cols_array();
            for (x, y) in a.iter().zip(b.iter()) {
                palette_delta = palette_delta.max((x - y).abs());
            }
        }
        // Skinning through the dense palette must reproduce canonical positions.
        let skins: Vec<GeosetSkin> = model.geosets.iter().map(GeosetSkin::from_geoset).collect();
        let mut skin_delta = 0.0f32;
        for (index, geoset) in model.geosets.iter().enumerate() {
            let a = skin_positions(geoset, &skins[index], &canonical);
            let b = skin_positions(geoset, &skins[index], &palette);
            for (p, q) in a.iter().zip(b.iter()) {
                skin_delta = skin_delta.max((*p - *q).length());
            }
        }
        let _ = runtime;

        let ok = pivots_ok
            && mismatches == 0
            && skin_kind_bad == 0
            && palette_delta < 1e-4
            && skin_delta < 1e-4;
        if ok {
            pass += 1;
        } else {
            fail += 1;
            failures.push(format!(
                "{}: pivots={node_objects}/{} mismatches={mismatches} bad_skin={skin_kind_bad} palette={palette_delta:.6} skin={skin_delta:.6}",
                file.display(),
                model.pivots.len()
            ));
        }
        let _ = writeln!(
            csv,
            "{},{bone_count},{},{node_objects},{mismatches},{max_skin},{palette_delta:.6},{skin_delta:.6},{}",
            file.display(),
            model.pivots.len(),
            if ok { "PASS" } else { "FAIL" }
        );
    }

    let out = root.join("analysis/agent-anim/audit/object-id-indexing.csv");
    let _ = std::fs::create_dir_all(out.parent().expect("audit dir"));
    std::fs::write(&out, csv).expect("write object id CSV");
    println!(
        "object-id indexing: {} models, PASS {pass} / FAIL {fail}, bones={total_bones} -> {}",
        files.len(),
        out.display()
    );
    for failure in &failures {
        println!("  FAIL {failure}");
    }
    assert_eq!(fail, 0, "{fail} models failed object-id indexing");
}

/// Canonical bone-index pose (bones then helpers) for comparison with the dense
/// object-id palette the game uses.
fn skeleton_pose(
    model: &MdxModel,
    sequence: &Sequence,
    time: u32,
    _animated: bool,
) -> Vec<glam::Mat4> {
    Skeleton::from_model(model).bone_worlds_in_sequence(sequence, time)
}
