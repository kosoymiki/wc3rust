//! Game-runtime-path sweep over **every** unit and building model.
//!
//! This is a library test of `wc3-anim` — the crate the game links — not the
//! audit CLI. It drives the same calls the game makes every frame:
//!
//! ```text
//! Animator::from_model(&model) -> play(sequence) -> update(dt) -> world_matrices()
//!     -> ModelRuntime::alphas / skin / visible
//! ```
//!
//! and asserts, per model:
//! * bone matrices are finite and change over 20 frames when the sequence has
//!   animated tracks,
//! * at least one geoset is visible on some frame,
//! * no geoset is drawn that the sequence extent marks absent,
//! * skinned vertices are finite.
//!
//! Run explicitly (walks 542 files):
//! `cargo test -p wc3-anim --test game_path_sweep -- --ignored --nocapture`

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use glam::Mat4;
use wc3_anim::{Animator, ModelRuntime};
use wc3_formats::mdx::{AnimValue, MdxModel, Sequence};

const FRAMES: usize = 20;

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

fn pick_sequence(model: &MdxModel, needles: &[&str]) -> Option<usize> {
    model.sequences.iter().enumerate().find_map(|(index, sequence)| {
        (sequence.end > sequence.start
            && needles
                .iter()
                .any(|needle| sequence.name.to_ascii_lowercase().contains(needle)))
        .then_some(index)
    })
}

fn has_animated_tracks(model: &MdxModel, sequence: &Sequence) -> bool {
    fn animated<T: AnimValue>(track: &wc3_formats::mdx::Track<T>, sequence: &Sequence) -> bool {
        track
            .keys
            .iter()
            .filter(|key| key.time >= sequence.start && key.time <= sequence.end)
            .count()
            >= 2
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

fn finite(matrix: &Mat4) -> bool {
    matrix.to_cols_array().iter().all(|value| value.is_finite())
}

#[test]
#[ignore = "walks the full extracted MPQ tree (542 models); run explicitly"]
fn every_model_animates_through_the_runtime() {
    let root = repo_root();
    let mut files = Vec::new();
    for tree in ["analysis/war3_full/Units", "analysis/war3_full/Buildings"] {
        walk_mdx(&root.join(tree), &mut files);
    }
    assert!(!files.is_empty(), "no MDX fixtures under analysis/war3_full");

    let mut csv = String::from(
        "model,kind,sequence,frames,moving_nodes,visible_frames,visible_max,absent_drawn,finite,verdict\n",
    );
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut failures = Vec::new();

    for file in &files {
        let Ok(bytes) = std::fs::read(file) else {
            continue;
        };
        let Ok(model) = MdxModel::parse(&bytes) else {
            failures.push(format!("{}: parse", file.display()));
            fail += 1;
            continue;
        };
        let kind = if file.to_string_lossy().to_ascii_lowercase().contains("portrait") {
            "portrait"
        } else if file.to_string_lossy().to_ascii_lowercase().contains("/buildings/") {
            "building"
        } else {
            "unit"
        };
        let runtime = ModelRuntime::new(&model);
        // Portraits have no world Stand; effects have no Stand at all.
        let sequence_index = if kind == "portrait" {
            pick_sequence(&model, &["portrait"]).or_else(|| pick_sequence(&model, &["stand"]))
        } else {
            pick_sequence(&model, &["stand"])
                .or_else(|| pick_sequence(&model, &["birth", "explode", "death"]))
        };
        let Some(sequence_index) = sequence_index else {
            // Particle-only models with no sequences: nothing to animate.
            csv.push_str(&format!("{},{},-,-,0,0,0,0,true,PASS\n", file.display(), kind));
            pass += 1;
            continue;
        };
        let sequence = &model.sequences[sequence_index];
        let animated = has_animated_tracks(&model, sequence);

        let mut animator = Animator::from_model(&model);
        animator.play(sequence);
        let step = (sequence.end - sequence.start).max(1) as f32 / FRAMES as f32 / 1000.0;
        let mut frames: Vec<Vec<Mat4>> = Vec::with_capacity(FRAMES);
        let mut times = Vec::with_capacity(FRAMES);
        for frame in 0..FRAMES {
            if frame > 0 {
                animator.update(step);
            }
            times.push(animator.frame_time());
            frames.push(animator.world_matrices());
        }

        let finite_all = frames.iter().all(|worlds| worlds.iter().all(finite));
        let bone_count = model.bones.len().min(frames[0].len());
        let node_count = frames[0].len();
        let mut moving = 0usize;
        for node in 0..node_count {
            let base = frames[0][node].to_cols_array();
            if frames.iter().any(|worlds| {
                worlds[node]
                    .to_cols_array()
                    .iter()
                    .zip(base.iter())
                    .any(|(a, b)| (a - b).abs() > 1e-3)
            }) {
                moving += 1;
            }
        }
        let mut visible_frames = 0usize;
        let mut visible_max = 0usize;
        let mut absent_drawn = 0usize;
        for time in &times {
            let alphas = runtime.alphas(&model, Some(sequence), *time);
            let count = alphas.iter().filter(|alpha| **alpha > 0.01).count();
            visible_frames += usize::from(count > 0);
            visible_max = visible_max.max(count);
            for (index, alpha) in alphas.iter().enumerate() {
                if *alpha > 0.01 && !model.geosets[index].present_in_sequence(sequence_index) {
                    absent_drawn += 1;
                }
            }
        }
        let bones = &frames[0][..bone_count];
        let mut skinned_finite = true;
        for geoset in 0..runtime.geoset_count {
            for position in runtime.skin(&model, geoset, bones) {
                if !position.is_finite() {
                    skinned_finite = false;
                }
            }
        }

        let mut verdict = "PASS";
        if !finite_all || !skinned_finite || absent_drawn > 0 || visible_max == 0 {
            verdict = "FAIL";
        }
        if animated && moving == 0 {
            verdict = "FAIL";
        }
        // Particle-only models legitimately have no geosets.
        if model.geosets.is_empty() {
            verdict = "PASS";
        }
        if verdict == "FAIL" {
            fail += 1;
            failures.push(format!(
                "{}: seq={} moving={moving} visible_max={visible_max} absent={absent_drawn} finite={finite_all}/{skinned_finite}",
                file.display(),
                sequence.name
            ));
        } else {
            pass += 1;
        }
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{},{},{},{}",
            file.display(),
            kind,
            sequence.name,
            FRAMES,
            moving,
            visible_frames,
            visible_max,
            absent_drawn,
            finite_all && skinned_finite,
            verdict
        );
    }

    let out = root.join("analysis/agent-anim/audit/game-path-sweep.csv");
    let _ = std::fs::create_dir_all(out.parent().expect("audit dir"));
    std::fs::write(&out, &csv).expect("write game-path CSV");
    println!(
        "game-path sweep: {} models, PASS {pass} / FAIL {fail} -> {}",
        files.len(),
        out.display()
    );
    for failure in &failures {
        println!("  FAIL {failure}");
    }
    assert_eq!(fail, 0, "{fail} models failed the runtime path");
}
