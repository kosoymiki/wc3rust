//! Deterministic animation check: a real unit's skinned node matrices must
//! change between frames, and the animator must actually be playing a sequence.
//!
//! This catches the class of regression where models load (meshes/sequences
//! present) but every frame renders the rest pose because the animation state
//! is never advanced or the bone array is empty.
use std::path::Path;

use wc3_game::{Assets, Game};

fn fixture() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let mpq = [
        "/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_iso/War3.mpq",
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
    ]
    .into_iter()
    .map(Path::new)
    .find(|path| path.is_file())?
    .to_path_buf();
    let map = Path::new(
        "/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_map/Maps/Campaign/Human04.w3m",
    )
    .to_path_buf();
    map.is_file().then_some((mpq, map))
}

#[test]
fn real_models_are_skinned_and_animate_over_time() {
    let Some((mpq, map)) = fixture() else {
        eprintln!("SKIP anim: user War3.mpq/map not present");
        return;
    };
    let assets = Assets::load(Some(&mpq), Some(&map));
    assert_eq!(assets.source, "user-mpq");

    for model in &assets.models {
        // Bounded placeholders (a def the map scope did not resolve) carry no
        // MDX by design; the animation invariant only applies to parsed models.
        if model.fallback {
            continue;
        }
        let bones = model
            .model
            .as_ref()
            .map(|mdx| mdx.bones.len())
            .unwrap_or(0);
        println!(
            "model {:?} bones={bones} sequences={} meshes={}",
            model.kind,
            model.sequences.len(),
            model.meshes.len()
        );
        assert!(bones > 0, "{:?} has no bones: animation impossible", model.kind);
        assert!(
            !model.sequences.is_empty(),
            "{:?} has no sequences: animation impossible",
            model.kind
        );
    }

    let mut game = Game::new(assets, 0x5EED);
    // The glue screens gate the sim: a game left on `Screen::MainMenu` never
    // ticks, so `scene()` would return draws with no animator state and every
    // assertion below would be vacuous. Enter the battle like the host does.
    game.enter_battle();
    game.resize(1280.0, 720.0);
    for _ in 0..5 {
        game.update(1.0 / 30.0);
    }

    let scene = game.scene();
    println!(
        "ANIM_STATE tick={} frames={} phase={:?} screen={:?} draws={} alive={}",
        game.sim.tick,
        game.frames,
        game.phase,
        game.screen,
        scene.units.len(),
        game.sim.alive_units().count()
    );
    assert!(
        game.sim.tick > 0,
        "the game never ticked (screen={:?}): the animation checks would be vacuous",
        game.screen
    );
    // Every skinned vertex must index a node that exists. Geoset skin indices
    // are MDX node object ids; if the animator builds its node array from the
    // BONE chunk order instead, models with interleaved attachments (Peasant)
    // map limbs to the wrong node and appear to float.
    for unit in &scene.units {
        let model = &game.assets.models[unit.model];
        // Static draws (the gold mine's `ngol`, a placement ghost) carry no bone
        // weights and no animator node array; the invariant is about skinned
        // vertices, so they are not part of it.
        if !model.is_skinned() {
            continue;
        }
        let pivots = model.model.as_ref().map(|mdx| mdx.pivots.len()).unwrap_or(0);
        let max_bone = model
            .meshes
            .iter()
            .flat_map(|mesh| mesh.bones.iter().flatten())
            .map(|bone| bone.bone as usize)
            .max()
            .unwrap_or(0);
        let mut ids_for_index: Vec<&str> = game
            .assets
            .model_by_def
            .iter()
            .filter(|(_, index)| **index == unit.model)
            .map(|(id, _)| id.as_str())
            .collect();
        ids_for_index.sort();
        let def_id = ids_for_index.first().copied().unwrap_or("?").to_string();
        println!(
            "{:?} def={def_id} ids={ids_for_index:?} index={} nodes={} pivots={} max_bone={max_bone} bones={} placeholder={}",
            model.kind,
            unit.model,
            unit.nodes.len(),
            pivots,
            model.model.as_ref().map(|mdx| mdx.bones.len()).unwrap_or(0),
            model.fallback
        );
        assert!(
            max_bone < unit.nodes.len(),
            "{:?} def={def_id} bone {max_bone} outside {} nodes",
            model.kind,
            unit.nodes.len()
        );
    }
    // Animate a skinned draw the scene actually contains. The campaign map's
    // opening army is `htow`/`hpea`/creeps, not a Footman: pick the draw with
    // the most bones and require it to advance between frames.
    let sample_index = scene
        .units
        .iter()
        .enumerate()
        .filter(|(_, draw)| {
            let model = &game.assets.models[draw.model];
            !model.fallback && draw.nodes.len() > 8
        })
        .max_by_key(|(_, draw)| draw.nodes.len())
        .map(|(index, _)| index)
        .expect("a skinned unit in the scene");
    let sample_model = scene.units[sample_index].model;
    let before: Vec<[f32; 16]> = scene.units[sample_index]
        .nodes
        .iter()
        .map(|m| m.to_cols_array())
        .collect();

    for _ in 0..20 {
        game.update(1.0 / 30.0);
    }
    let scene = game.scene();
    let after_unit = scene
        .units
        .iter()
        .find(|unit| unit.model == sample_model)
        .expect("the same skinned unit");
    assert_eq!(
        before.len(),
        after_unit.nodes.len(),
        "node count changed between frames"
    );
    let mut changed = 0usize;
    let mut max_delta = 0.0f32;
    for (a, b) in before.iter().zip(&after_unit.nodes) {
        let b = b.to_cols_array();
        let delta = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        if delta > 1.0e-4 {
            changed += 1;
        }
        max_delta = max_delta.max(delta);
    }
    println!("animation: nodes={} changed={changed} max_delta={max_delta:.6}", before.len());
    assert!(
        changed > 0,
        "no bone matrix changed over 20 frames: models render in rest pose"
    );
    assert!(
        max_delta > 1.0e-3,
        "bone deltas too small to see (max {max_delta:.6})"
    );
}
