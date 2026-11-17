//! Hero ability and projectile/missile sequences through the canonical
//! [`AnimatorRegistry`].
//!
//! The registry owns the alias table (`AbilitySequence`, `ProjectileSequence`)
//! so the game and the gallery tooling address gameplay animations the same
//! way: `ability_animator("Units\\Orc\\HeroBladeMaster\\HeroBladeMaster.mdx",
//! AbilitySequence::Cast)` returns an independent animator already playing the
//! Bladestorm (`Attack Walk Stand Spin`) sequence.
//!
//! These tests walk the shipped model tree and prove, for every hero and every
//! missile model:
//! * cast/channel and flight/impact sequences resolve (fallbacks flagged),
//! * the sequence animates nodes over its own time range,
//! * skinned vertices stay finite and inside model space.

use std::path::{Path, PathBuf};

use wc3_anim::{
    AbilitySequence, AnimatorRegistry, GeosetSkin, ProjectileSequence, skin_positions,
};
use wc3_formats::mdx::MdxModel;

const MAX_ABSOLUTE_POSITION: f32 = 4096.0;

fn assets_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["analysis/war3_full", "analysis/war3_samples"]
        .iter()
        .map(|tree| root.join(tree))
        .find(|candidate| candidate.join("Units/UnitData.slk").is_file())
}

fn walk_mdx(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_mdx(&path, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("mdx"))
            == Some(true)
        {
            out.push(path);
        }
    }
}

/// Actual unit models in `Units/*/Hero*`, portraits excluded. Matching uses
/// the path relative to the asset root: the repository directory name itself
/// contains "hero", so a full-path filter would sweep every unit.
fn hero_models(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_mdx(&root.join("Units"), &mut files);
    files.retain(|path| {
        let relative = relative_key(root, path).to_ascii_lowercase();
        relative.contains("/hero") && !relative.contains("portrait")
    });
    files.sort();
    files
}

/// Missile models: both case variants of the shipped `Abilities/Weapons`
/// directory, deduplicated by lower-cased relative path.
fn projectile_models(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for tree in ["Abilities/Weapons", "abilities/Weapons"] {
        let dir = root.join(tree);
        if dir.is_dir() {
            walk_mdx(&dir, &mut files);
        }
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|path| {
        let key = relative_key(root, path).to_ascii_lowercase();
        seen.insert(key)
    });
    files.sort();
    files
}

fn relative_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Largest absolute matrix component change across 16 samples of one sequence.
fn sequence_motion(animator: &mut wc3_anim::Animator, length_ms: u32) -> f32 {
    const FRAMES: usize = 16;
    let step_ms = length_ms.max(1) as f32 / FRAMES as f32;
    let reference = animator.world_matrices();
    let mut max = 0.0f32;
    for _ in 1..FRAMES {
        animator.update(step_ms / 1000.0);
        for (world, base) in animator.world_matrices().iter().zip(reference.iter()) {
            for (a, b) in world.to_cols_array().iter().zip(base.to_cols_array().iter()) {
                max = max.max((a - b).abs());
            }
        }
    }
    max
}

/// Skinning smoke: every skinned vertex must stay finite and in model space.
/// Returns the number of vertices checked.
fn checked_vertices(model: &MdxModel, animator: &wc3_anim::Animator) -> usize {
    let bone_worlds = animator.world_matrices();
    let mut checked = 0usize;
    for (index, geoset) in model.geosets.iter().enumerate() {
        let skin = GeosetSkin::from_geoset(geoset);
        for position in skin_positions(geoset, &skin, &bone_worlds) {
            assert!(
                position.is_finite(),
                "non-finite skinned vertex in geoset {index}"
            );
            assert!(
                position.length() < MAX_ABSOLUTE_POSITION,
                "skinned vertex escaped model space: {position} (geoset {index})"
            );
            checked += 1;
        }
    }
    checked
}

fn load(registry: &mut AnimatorRegistry, root: &Path, path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let model = MdxModel::parse(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error:?}", path.display()));
    let key = relative_key(root, path);
    registry.insert(&key, model);
    key
}

#[test]
fn heroes_expose_cast_and_channel_through_registry() {
    let Some(root) = assets_root() else {
        eprintln!("skipping: shipped model tree missing");
        return;
    };
    let heroes = hero_models(&root);
    assert!(
        heroes.len() >= 15,
        "expected at least 15 hero models, found {}",
        heroes.len()
    );
    let mut registry = AnimatorRegistry::new(32);
    let mut exact_cast = 0usize;
    let mut fallback_cast = 0usize;
    let mut fallback_channel = Vec::new();
    for path in &heroes {
        let key = load(&mut registry, &root, path);
        let cast = registry
            .ability_sequence(&key, AbilitySequence::Cast)
            .unwrap_or_else(|| panic!("no cast sequence for {key}"));
        if cast.is_exact() {
            exact_cast += 1;
        } else {
            fallback_cast += 1;
            eprintln!("cast fallback {key}: {}", cast.sequence.name);
        }
        let channel = registry
            .ability_sequence(&key, AbilitySequence::Channel)
            .unwrap_or_else(|| panic!("no channel sequence for {key}"));
        if channel.fallback {
            fallback_channel.push(key.clone());
        }
        // Death and dissipate must resolve for every shipped hero.
        assert!(
            registry.ability_sequence(&key, AbilitySequence::Death).is_some(),
            "no death sequence for {key}"
        );
        assert!(
            registry
                .ability_sequence(&key, AbilitySequence::Dissipate)
                .is_some(),
            "no dissipate sequence for {key}"
        );
        assert!(
            registry.sequence_names(&key).map_or(false, |names| !names.is_empty()),
            "no playable sequences for {key}"
        );
    }
    eprintln!(
        "heroes: {} models, cast exact={exact_cast} fallback={fallback_cast}, channel fallbacks={} {:?}",
        heroes.len(),
        fallback_channel.len(),
        fallback_channel
    );
    assert_eq!(fallback_cast, 0, "every hero authors a canonical cast animation");
    assert_eq!(registry.len(), heroes.len());
}

#[test]
fn hero_cast_animates_and_skins_finite() {
    let Some(root) = assets_root() else {
        eprintln!("skipping: shipped model tree missing");
        return;
    };
    let heroes = hero_models(&root);
    let mut registry = AnimatorRegistry::new(32);
    let mut moved = 0usize;
    let mut checked = 0usize;
    for path in &heroes {
        let key = load(&mut registry, &root, path);
        let (cast, mut animator) = registry
            .ability_animator(&key, AbilitySequence::Cast)
            .unwrap_or_else(|| panic!("no cast animator for {key}"));
        let length = cast.sequence.end - cast.sequence.start;
        let motion = sequence_motion(&mut animator, length);
        if motion > 1e-4 {
            moved += 1;
        } else {
            eprintln!("static cast? {key} ({})", cast.sequence.name);
        }
        checked += registry
            .with_runtime(&key, |model, _runtime| {
                checked_vertices(model, &animator)
            })
            .unwrap_or(0);
    }
    eprintln!(
        "hero casts: {} models, moving={moved}, skinned_vertices_checked={checked}",
        heroes.len()
    );
    assert_eq!(moved, heroes.len(), "every hero cast animation must move nodes");
    assert!(checked > 0, "skinning check must cover vertices");
}

#[test]
fn projectiles_expose_flight_and_impact_and_animate() {
    let Some(root) = assets_root() else {
        eprintln!("skipping: shipped model tree missing");
        return;
    };
    let missiles = projectile_models(&root);
    assert!(
        missiles.len() >= 50,
        "expected at least 50 missile models, found {}",
        missiles.len()
    );
    let mut registry = AnimatorRegistry::new(64);
    let mut fallback_flight = Vec::new();
    let mut fallback_impact = Vec::new();
    let mut exact_flight = 0usize;
    let mut exact_impact = 0usize;
    let mut moving_flight = 0usize;
    let mut moving_impact = 0usize;
    let mut checked = 0usize;
    for path in &missiles {
        let key = load(&mut registry, &root, path);
        let (flight, mut animator) = registry
            .projectile_animator(&key, ProjectileSequence::Flight)
            .unwrap_or_else(|| panic!("no flight sequence for {key}"));
        if flight.fallback {
            fallback_flight.push(format!("{key} -> {}", flight.sequence.name));
        } else {
            exact_flight += 1;
        }
        if sequence_motion(&mut animator, flight.sequence.end - flight.sequence.start) > 1e-4 {
            moving_flight += 1;
        }
        let (impact, mut animator) = registry
            .projectile_animator(&key, ProjectileSequence::Impact)
            .unwrap_or_else(|| panic!("no impact sequence for {key}"));
        if impact.fallback {
            fallback_impact.push(format!("{key} -> {}", impact.sequence.name));
        } else {
            exact_impact += 1;
        }
        if sequence_motion(&mut animator, impact.sequence.end - impact.sequence.start) > 1e-4 {
            moving_impact += 1;
        }
        checked += registry
            .with_runtime(&key, |model, _runtime| {
                checked_vertices(model, &animator)
            })
            .unwrap_or(0);
    }
    eprintln!(
        "missiles: {} models, flight exact={exact_flight} fallbacks={fallback_flight:?}, impact exact={exact_impact} fallbacks={fallback_impact:?}, moving flight={moving_flight} impact={moving_impact}, skinned_vertices_checked={checked}",
        missiles.len()
    );
    // Data shape (observed 2026-11-13): 67 missile models; particle missiles
    // authored only as Birth/Death fall back for flight, impact-only effects
    // (BallistaImpact, RifleImpact, ...Target) fall back to their Birth puff.
    assert!(
        exact_flight >= 50,
        "expected >= 50 missiles with an authored flight sequence, got {exact_flight}"
    );
    assert!(
        exact_impact >= 50,
        "expected >= 50 missiles with an authored impact sequence, got {exact_impact}"
    );
    assert!(
        moving_flight + moving_impact >= missiles.len(),
        "missile sequences must animate at least one of flight/impact"
    );
    assert!(checked > 0, "skinning check must cover missile vertices");
}
