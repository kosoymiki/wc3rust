//! Model residency: the asset layer parses the MDX of the definitions the
//! current map can show, not the whole 814-row roster.
//!
//! `mem_probe --breakdown` attributed 169.8 MB of native heap to the parsed
//! `MdxModel` of every roster definition (bones, node tracks, sequences,
//! materials) on a device whose scene shows ~150 of them. These tests pin the
//! map scope, the missing-model path and the campaign script path on the real
//! install:
//!
//! * `assets.models-are-map-scoped` — Justice loads <= 250 models while all 100
//!   spawned scene units resolve a real (non-placeholder) model;
//! * `assets.unreachable-def-placeholder` — a def outside the set resolves to a
//!   bounded box and logs `GAME_MODEL_SKIP` exactly once;
//! * `assets.campaign-script-ids` — a campaign map keeps the units its
//!   `war3map.j` creates.
use std::path::{Path, PathBuf};

use wc3_game::assets::{Assets, ModelScopeMode};
use wc3_game::game::Game;

fn mpq() -> Option<PathBuf> {
    [
        "/data/data/org.herofolends.p0/files/War3.mpq",
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_iso/War3.mpq",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

fn justice() -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    path.is_file().then_some(path)
}

fn campaign() -> Option<PathBuf> {
    [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_map/Maps/Campaign/Human04.w3m"),
        PathBuf::from("/data/data/org.herofolends.p0/files/Human04.w3m"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

/// `assets.models-are-map-scoped`: the Justice load resolves only the defs that
/// map can show, and every unit the scene spawns still has its own MDX.
#[test]
fn assets_models_are_map_scoped_justice() {
    let (Some(mpq), Some(map)) = (mpq(), justice()) else {
        eprintln!("skip: War3.mpq or Justice.w3x missing");
        return;
    };
    let mut game = Game::new(Assets::load(Some(&mpq), Some(&map)), 0x5EED);
    for _ in 0..10 {
        game.update(1.0 / 30.0);
    }
    let assets = &game.assets;
    eprintln!(
        "MODELS justice {} models={} roster={} placeholders={}",
        assets.model_scope.summary(),
        assets.loaded_model_count(),
        assets.model_roster,
        assets.placeholder_count()
    );
    assert_eq!(
        assets.model_scope.mode,
        ModelScopeMode::MapScoped,
        "Justice has placements + w3i seats, so the load must be map-scoped: {}",
        assets.model_scope.summary()
    );
    assert!(
        assets.model_roster >= 700,
        "the object database carries the full roster, got {}",
        assets.model_roster
    );
    let loaded = assets.loaded_model_count();
    assert!(
        loaded <= 250,
        "map-scoped Justice parses {loaded} MDX models, budget is 250 (roster {})",
        assets.model_roster
    );
    assert!(
        loaded < assets.model_roster,
        "a scoped load must parse fewer models than the roster: loaded={loaded} roster={}",
        assets.model_roster
    );

    // Every spawned unit — 88 `war3mapUnits.doo` placements, 2 halls and 10
    // workers of the w3i race kit — must resolve its own MDX, not the box.
    let mut placeholders = Vec::new();
    let mut checked = 0usize;
    for unit in game.sim.alive_units() {
        checked += 1;
        let def = unit.def().id;
        let model = assets.model_for_def(def);
        if model.fallback || model.model.is_none() {
            placeholders.push(format!("{def} height={:.0}", model.height));
        }
    }
    eprintln!("MODELS scene_units={checked} placeholders={placeholders:?}");
    assert!(
        checked >= 100,
        "the Justice scene spawns 88 map units + 2 halls + 10 workers, got {checked}"
    );
    assert!(
        placeholders.is_empty(),
        "every spawned unit must draw its own model, placeholders: {placeholders:?}"
    );

    // The draw list the renderer consumes carries the same promise.
    let scene = game.scene();
    let boxed = scene
        .units
        .iter()
        .filter(|draw| assets.models[draw.model].fallback)
        .count();
    eprintln!("MODELS scene_draws={} placeholders={boxed}", scene.units.len());
    assert_eq!(boxed, 0, "no scene draw may use a placeholder model");
}

/// `assets.unreachable-def-placeholder`: a def outside the map scope resolves to
/// a bounded placeholder and logs `GAME_MODEL_SKIP` once per id.
#[test]
fn assets_unreachable_def_resolves_to_placeholder_once() {
    let (Some(mpq), Some(map)) = (mpq(), justice()) else {
        eprintln!("skip: War3.mpq or Justice.w3x missing");
        return;
    };
    let mut assets = Assets::load(Some(&mpq), Some(&map));
    // Undead acolyte: Justice has no Undead seat, places no acolyte and no
    // reachable producer trains one.
    let def = "uaco";    assert!(
        !assets.model_scope.wants(def),
        "uaco must be outside the Justice scope: {}",
        assets.model_scope.summary()
    );
    assert!(assets.is_placeholder(def), "uaco resolved a real model");
    let first = assets.model_for_def(def);
    assert!(first.fallback, "unreachable def must return the placeholder");
    assert!(first.model.is_none(), "placeholder carries no parsed MDX");
    assert!(!first.meshes.is_empty(), "placeholder has geometry");
    assert!(
        (40.0..=300.0).contains(&first.height),
        "placeholder height is bounded, got {}",
        first.height
    );
    let height = first.height;
    // Many frames ask for the same def: the log line stays single.
    for _ in 0..3 {
        assert_eq!(assets.model_for_def(def).height, height);
    }
    let log = assets.model_skip_log();
    let hits: Vec<&String> = log.iter().filter(|line| line.contains("def=uaco")).collect();
    eprintln!("MODELS skip_log={log:?}");
    assert_eq!(hits.len(), 1, "one line per skipped id: {log:?}");
    assert_eq!(
        hits[0].as_str(),
        "GAME_MODEL_SKIP def=uaco reason=not-reachable"
    );

    // A def with no data row is explicit about that, not silently reused.
    let unknown = assets.model_for_def("zzzz");
    assert!(unknown.fallback, "unknown def must return the placeholder");
    assert!(
        assets
            .model_skip_log()
            .iter()
            .any(|line| line == "GAME_MODEL_SKIP def=zzzz reason=unknown-def"),
        "unknown def gets its own reason"
    );

    // Reachable defs of the same load are real models (positive control).
    for id in ["hfoo", "opeo", "ogre", "hpea", "oalt"] {
        assert!(
            !assets.is_placeholder(id),
            "{id} is reachable on Justice and must be resident"
        );
    }

    // On-demand path: a def that only becomes known later can be parsed
    // explicitly, before the renderer snapshots the model table. The
    // placeholder slot is reused, so no model index moves.
    let loaded_before = assets.loaded_model_count();
    let placeholders_before = assets.placeholder_count();
    let entries_before = assets.models.len();
    assert!(
        assets.ensure_model(def),
        "ensure_model must parse the acolyte's MDX from the install"
    );
    assert!(
        !assets.is_placeholder(def),
        "after ensure_model the acolyte has its own model"
    );
    assert_eq!(
        assets.loaded_model_count(),
        loaded_before + 1,
        "on-demand load adds exactly one MDX-backed model"
    );
    assert_eq!(
        assets.models.len(),
        entries_before,
        "the placeholder slot is replaced in place, not appended"
    );
    assert_eq!(
        assets.placeholder_count(),
        placeholders_before - 1,
        "one bounded placeholder is gone once the real MDX is resident"
    );
    assert!(
        assets.ensure_model(def),
        "a second ensure_model call is a no-op success"
    );
    assert_eq!(
        assets.loaded_model_count(),
        loaded_before + 1,
        "no duplicate entry on the second call"
    );
}

/// `assets.campaign-script-ids`: a campaign map owns units through its script;
/// the load must keep those ids (map-scoped with script ids, or the explicit
/// full-roster fallback when the script creates units through variables).
#[test]
fn assets_campaign_script_ids_are_kept() {
    let (Some(mpq), Some(map)) = (mpq(), campaign()) else {
        eprintln!("skip: War3.mpq or Human04.w3m missing");
        return;
    };
    let assets = Assets::load(Some(&mpq), Some(&map));
    eprintln!(
        "MODELS campaign {} models={} roster={} placeholders={}",
        assets.model_scope.summary(),
        assets.loaded_model_count(),
        assets.model_roster,
        assets.placeholder_count()
    );
    // `CreateUnitsForPlayer1` creates footmen/peasants by literal id.
    assert!(
        assets.model_scope.ids.contains("hfoo"),
        "script-created ids join the scope: {}",
        assets.model_scope.summary()
    );
    assert!(
        !assets.is_placeholder("hfoo"),
        "the script's footman model is resident"
    );
    match assets.model_scope.mode {
        ModelScopeMode::MapScoped => {
            assert!(
                assets.loaded_model_count() <= 400,
                "campaign scope stayed small: {}",
                assets.loaded_model_count()
            );
        }
        ModelScopeMode::FullRoster => {
            assert!(
                assets.model_scope.reason.starts_with("campaign-script"),
                "a full-roster campaign fallback must say why: {}",
                assets.model_scope.reason
            );
            assert!(
                assets.loaded_model_count() > 400,
                "the fallback loads the roster: {}",
                assets.loaded_model_count()
            );
        }
    }
    // Every id the map places or its script creates resolves its own model.
    let missing: Vec<&str> = assets
        .model_scope
        .ids
        .iter()
        .map(String::as_str)
        .filter(|id| wc3_sim::data::def_of(id).is_some() && assets.is_placeholder(id))
        .collect();
    assert!(
        missing.is_empty(),
        "campaign ids without a model: {missing:?}"
    );
}
