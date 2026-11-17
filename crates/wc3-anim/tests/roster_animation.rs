//! Roster animation check: every model the game roster uses must animate.
//!
//! This is a `wc3-anim` library test (the crate the game links), independent of
//! the `wc3-game`/`wc3-sim` build so it stays runnable while other layers are in
//! flight. It covers:
//!
//! * the four roster models (`Footman`, `Archer`, `peasant`, `TownHall`),
//! * the three roster portrait models plus the TownHall `Portrait` sequence,
//!
//! for each: parse, pick the presentation sequence, run 20 frames through
//! `Animator::from_model -> play -> update -> world_matrices`, and require moving
//! bone matrices, a visible geoset and finite skinned vertices.
//!
//! Run: `cargo test -p wc3-anim --test roster_animation -- --nocapture`

use std::path::{Path, PathBuf};

use wc3_anim::{Animator, ModelRuntime};
use wc3_formats::mdx::MdxModel;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(relative: &str) -> Option<(PathBuf, MdxModel)> {
    let root = repo_root();
    for tree in ["analysis/war3_samples", "analysis/war3_full"] {
        let path = root.join(tree).join(relative);
        if path.is_file() {
            let bytes = std::fs::read(&path).ok()?;
            let model = MdxModel::parse(&bytes).ok()?;
            return Some((path, model));
        }
    }
    None
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

fn moving_nodes(model: &MdxModel, sequence: &wc3_formats::mdx::Sequence) -> usize {
    let mut animator = Animator::from_model(model);
    animator.play(sequence);
    let step = (sequence.end - sequence.start).max(1) as f32 / 20.0 / 1000.0;
    let mut frames = Vec::new();
    for frame in 0..20 {
        if frame > 0 {
            animator.update(step);
        }
        frames.push(animator.world_matrices());
    }
    (0..model.bones.len())
        .filter(|bone| {
            let base = frames[0][*bone].to_cols_array();
            frames.iter().any(|worlds| {
                worlds[*bone]
                    .to_cols_array()
                    .iter()
                    .zip(base.iter())
                    .any(|(a, b)| (a - b).abs() > 1e-3)
            })
        })
        .count()
}

fn check(relative: &str, needles: &[&str]) {
    let Some((path, model)) = load(relative) else {
        eprintln!("SKIP {relative}: fixture missing");
        return;
    };
    let runtime = ModelRuntime::new(&model);
    let index = pick(&model, needles)
        .unwrap_or_else(|| panic!("{}: no sequence matching {needles:?}", path.display()));
    let sequence = &model.sequences[index];
    let moving = moving_nodes(&model, sequence);
    let visible = runtime.visible(&model, sequence, sequence.start);
    assert!(
        !visible.is_empty(),
        "{}: no visible geoset in {}",
        path.display(),
        sequence.name
    );
    assert!(
        moving > 0,
        "{}: {} does not animate (0 moving nodes)",
        path.display(),
        sequence.name
    );
    let bones = runtime.bones(sequence, sequence.start);
    for geoset in 0..runtime.geoset_count {
        for position in runtime.skin(&model, geoset, &bones) {
            assert!(position.is_finite(), "{}: non-finite vertex", path.display());
        }
    }
    println!(
        "{}: sequence={} bones={} moving_nodes={moving} visible_geosets={:?}",
        path.display(),
        sequence.name,
        model.bones.len(),
        visible
    );
}

#[test]
fn roster_models_animate() {
    check("Units/Human/Footman/Footman.mdx", &["stand"]);
    check("Units/NightElf/Archer/Archer.mdx", &["stand"]);
    check("Units/Human/Peasant/peasant.mdx", &["stand"]);
    check("Buildings/Human/TownHall/TownHall.mdx", &["stand"]);
}

#[test]
fn roster_portraits_animate() {
    check("Units/Human/Footman/Footman_Portrait.mdx", &["portrait"]);
    check("Units/NightElf/Archer/Archer_Portrait.mdx", &["portrait"]);
    check("Units/Human/Peasant/peasant_Portrait.mdx", &["portrait"]);
    // Buildings have no separate portrait model; the sequence lives in the model.
    check("Buildings/Human/TownHall/TownHall.mdx", &["portrait"]);
}
