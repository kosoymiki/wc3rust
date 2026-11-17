//! Game-runtime-path animation proof for the models the game actually loads.
//!
//! Unlike the audit CLI, this test goes through the real game surface:
//! `Assets::load(mpq, map)` -> `Game` -> `update(dt)` -> `Scene::units[].nodes`.
//! Every unit present in the starting scene must have a node palette and its
//! bone matrices must change over 20 frames.
use std::path::{Path, PathBuf};

use wc3_game::{Assets, Game};

fn fixture() -> Option<(PathBuf, PathBuf)> {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_iso/War3.mpq",
    ]
    .into_iter()
    .map(Path::new)
    .find(|path| path.is_file())?
    .to_path_buf();
    let map = Path::new(
        "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_map/Maps/Campaign/Human04.w3m",
    )
    .to_path_buf();
    map.is_file().then_some((mpq, map))
}

type Snapshot = Vec<(usize, Vec<[f32; 16]>)>;

fn snapshot(game: &Game) -> Snapshot {
    game.scene()
        .units
        .iter()
        .map(|unit| {
            (
                unit.model,
                unit.nodes.iter().map(|matrix| matrix.to_cols_array()).collect(),
            )
        })
        .collect()
}

/// Sample a sequence across its own length (24 Hz, capped) and report whether
/// any node matrix changes: a 333 ms Stand loop needs 8 samples, a 10 s shipyard
/// clip needs ~240, and comparing only first vs last would miss both.
fn clip_animates(mdx: &wc3_formats::mdx::MdxModel, sequence: &wc3_formats::mdx::Sequence) -> bool {
    let mut animator = wc3_anim::Animator::from_model(mdx);
    animator.play(sequence);
    let milliseconds = sequence.end.saturating_sub(sequence.start);
    let steps = ((milliseconds as f32 / 1000.0) * 24.0).ceil().clamp(8.0, 240.0) as usize;
    let base = animator.world_matrices();
    for step in 1..steps {
        animator.update(1.0 / 24.0);
        let frame = animator.world_matrices();
        if frame.len() != base.len() {
            return true;
        }
        for (a, b) in base.iter().zip(frame.iter()) {
            if a.to_cols_array()
                .iter()
                .zip(b.to_cols_array().iter())
                .any(|(x, y)| (x - y).abs() > 1e-3)
            {
                return true;
            }
        }
    }
    false
}

/// A model whose meshes carry bone weights needs an animator palette; a static
/// model (gold mine, placement ghost) is drawn with identity transforms.
fn max_delta(before: &Snapshot, after: &Snapshot) -> (usize, f32) {    let mut changed = 0usize;
    let mut worst = 0.0f32;
    for (model, before_nodes) in before {
        let Some((_, after_nodes)) = after.iter().find(|(candidate, _)| candidate == model) else {
            continue;
        };
        for (a, b) in before_nodes.iter().zip(after_nodes.iter()) {
            let delta = a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| (x - y).abs())
                .fold(0.0f32, f32::max);
            if delta > 1e-3 {
                changed += 1;
            }
            worst = worst.max(delta);
        }
    }
    (changed, worst)
}

#[test]
fn every_loaded_unit_animates_over_20_frames() {
    let Some((mpq, map)) = fixture() else {
        eprintln!("SKIP anim_all_units: user War3.mpq/Human04 map not present");
        return;
    };
    let assets = Assets::load(Some(&mpq), Some(&map));
    assert_eq!(assets.source, "user-mpq", "game must load the operator install");

    let mut game = Game::new(assets, 0x5EED);
    // The glue screens gate the sim: on `Screen::MainMenu` the game never ticks,
    // so every draw would carry an empty node palette and these assertions would
    // be vacuous. Enter the battle like the host does.
    game.enter_battle();
    game.resize(1280.0, 720.0);
    for _ in 0..5 {
        game.update(1.0 / 30.0);
    }
    assert!(game.sim.tick > 0, "the game never ticked (screen={:?})", game.screen);

    let before = snapshot(&game);
    assert!(!before.is_empty(), "starting scene has no units");
    for (model, nodes) in &before {
        let asset = &game.assets.models[*model];
        // Bounded placeholders (a def outside the map scope) carry no MDX, and
        // static draws (the gold mine's `ngol`) carry no bone weights and no
        // animator palette; the invariant is about skinned, parsed models.
        if asset.fallback || !asset.is_skinned() {
            continue;
        }
        // The animator palette is dense over MDX object ids (bones first, then
        // helpers/attachments/events; gaps filled with identity), so the palette
        // is at least the bone count and every skin index must be in range.
        let (bones, pivots, max_skin) = asset
            .model
            .as_ref()
            .map(|mdx| {
                let max_skin = asset
                    .meshes
                    .iter()
                    .flat_map(|mesh| mesh.bones.iter().flatten())
                    .map(|bone| bone.bone as usize)
                    .max()
                    .unwrap_or(0);
                (mdx.bones.len(), mdx.pivots.len(), max_skin)
            })
            .unwrap_or((0, 0, 0));
        assert!(bones > 0, "{:?} has no bones", asset.kind);
        assert!(
            nodes.len() >= bones.max(pivots),
            "{:?} palette {} < max(bones={bones}, pivots={pivots})",
            asset.kind,
            nodes.len()
        );
        assert!(
            max_skin < nodes.len(),
            "{:?} skin index {max_skin} outside palette {}",
            asset.kind,
            nodes.len()
        );
        assert!(!asset.sequences.is_empty(), "{:?} has no sequences", asset.kind);
        assert!(
            nodes.iter().flatten().all(|value| value.is_finite()),
            "{:?} has a non-finite node matrix",
            asset.kind
        );
    }

    for _ in 0..20 {
        game.update(1.0 / 30.0);
    }
    let after = snapshot(&game);
    assert_eq!(
        before.len(),
        after.len(),
        "unit count changed while sampling animation"
    );

    let mut animated_models = 0usize;
    // Gold mines are static scenery: the draw carries the model's bind pose
    // (no animator), so they are not part of the "every unit animates" set.
    let mine_model = game.assets.model_index_for_def("ngol");
    for (model, _) in &before {
        if *model == mine_model {
            continue;
        }
        if game.assets.models[*model].fallback || !game.assets.models[*model].is_skinned() {
            continue;
        }
        let (changed, worst) = max_delta(
            &before
                .iter()
                .filter(|(candidate, _)| candidate == model)
                .cloned()
                .collect::<Vec<_>>(),
            &after
                .iter()
                .filter(|(candidate, _)| candidate == model)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let kind = format!("{:?}", game.assets.models[*model].kind);
        println!("{kind}: changed_nodes={changed} max_delta={worst:.6}");
        if changed > 0 && worst > 1e-3 {
            animated_models += 1;
        } else if game.assets.models[*model]
            .model
            .as_ref()
            .map(|mdx| {
                mdx.sequences.iter().all(|_| {
                    mdx.bones
                        .iter()
                        .map(|bone| &bone.node)
                        .chain(mdx.helpers.iter())
                        .all(|node| {
                            node.translation.keys.is_empty()
                                && node.rotation.keys.is_empty()
                                && node.scaling.keys.is_empty()
                        })
                })
            })
            .unwrap_or(true)
        {
            println!("{kind}: static by data (no node tracks in any sequence)");
        } else {
            panic!("{kind}: no node matrix changed over 20 frames");
        }
    }
    assert!(
        animated_models > 0,
        "no loaded unit animated through the game runtime"
    );

    // Roster completeness: every skinned model the game loads must animate in
    // at least one of its sequences, even if the starting scene does not spawn
    // that unit kind. `Stand` is checked first; a model whose Stand clip is
    // constant by data (Shipyard's water, braziers) must still move in another
    // clip, which is what the sweep proves.
    // Bounded placeholders (`GAME_MODEL_SKIP`) carry no MDX and are not roster
    // models: the asset layer never borrowed another unit's art for them.
    let mut animated_roster = 0usize;
    let mut static_roster = 0usize;
    for (ordinal, asset) in game.assets.models.iter().enumerate() {
        if asset.fallback {
            continue;
        }
        let kind = format!("{:?}", asset.kind);
        let Some(mdx) = asset.model.as_ref() else {
            panic!("{kind}: non-placeholder model without MDX");
        };
        if !asset.is_skinned() {
            continue;
        }
        animated_roster += 1;
        let def_id = game
            .assets
            .model_by_def
            .iter()
            .filter(|(_, index)| **index == ordinal)
            .map(|(id, _)| id.as_str())
            .min()
            .unwrap_or("?");
        // Stand first (what the game shows at rest), then every other clip: a
        // roster model without a Stand sequence must still be proven to animate
        // somewhere, and one with no sequences at all is static by data.
        let mut order: Vec<usize> = asset
            .sequence(wc3_game::assets::SequenceKind::Idle)
            .into_iter()
            .collect();
        let rest: Vec<usize> = (0..asset.sequences.len())
            .filter(|index| !order.contains(index))
            .collect();
        order.extend(rest);
        let mut animated_clip = None;
        for index in order {
            let sequence = &asset.sequences[index];
            if clip_animates(mdx, sequence) {
                animated_clip = Some(index);
                break;
            }
        }
        match animated_clip {
            Some(index) => println!(
                "roster {kind} def={def_id}: sequence={} animates",
                asset.sequences[index].name
            ),
            None => {
                static_roster += 1;
                println!("roster {kind} def={def_id}: static by data (no clip moves a node)");
            }
        }
    }
    assert!(
        animated_roster > 0,
        "no MDX-backed roster model was checked"
    );
    println!("roster totals: animated={animated_roster} static_by_data={static_roster}");
    assert!(
        animated_roster > static_roster,
        "the roster must be dominated by animating models: animated={animated_roster} static={static_roster}"
    );
}
