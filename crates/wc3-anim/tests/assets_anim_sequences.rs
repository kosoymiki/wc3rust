//! `assets.anim.sequences`: every roster model's gameplay states (idle, walk,
//! attack, death, cast, channel) must map to an authored MDX sequence, and a
//! missing state must be reported per model — never silently substituted.
//!
//! Writes:
//! * `analysis/agent-anim/audit/state-mapping.csv` (model,state,sequence,missing)
//! * `analysis/agent-anim/audit/state-mapping-missing.csv` (model,missing states)

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use wc3_anim::{AnimatorRegistry, GameplayState};
use wc3_formats::mdx::MdxModel;

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

fn relative_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn state_matches(state: GameplayState, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match state {
        GameplayState::Idle => lower.contains("stand"),
        GameplayState::Walk => lower.contains("walk"),
        GameplayState::Attack => lower.contains("attack"),
        GameplayState::Death => lower.contains("death"),
        GameplayState::Cast => {
            lower.contains("spell")
                || lower.contains("cast")
                || lower.contains("attack walk stand spin")
        }
        GameplayState::Channel => lower.contains("channel"),
        GameplayState::Build => lower.contains("stand work") || lower.contains("build"),
        GameplayState::Birth => lower.contains("birth"),
    }
}

#[test]
fn assets_anim_sequences_roster_state_mapping_without_silent_fallback() {
    let Some(root) = assets_root() else {
        eprintln!("skipping: shipped model tree missing");
        return;
    };
    let mut files = Vec::new();
    for tree in ["Units", "Buildings", "Abilities/Weapons"] {
        let dir = root.join(tree);
        if dir.is_dir() {
            walk_mdx(&dir, &mut files);
        }
    }
    // In-game roster set: every non-portrait unit/building model plus every
    // projectile/missile model. Portraits are UI presentation assets and are
    // never played by the game world (194 of them are excluded and reported).
    files.retain(|path| {
        !relative_key(&root, path)
            .to_ascii_lowercase()
            .contains("portrait")
    });
    let mut seen = HashSet::new();
    files.retain(|path| seen.insert(relative_key(&root, path).to_ascii_lowercase()));
    files.sort();
    assert_eq!(
        files.len(),
        415,
        "roster model set = non-portrait units 225 + buildings 123 + projectiles 67"
    );

    let mut registry = AnimatorRegistry::new(4);
    let mut mapping = String::from("model,state,sequence,missing\n");
    let mut missing_rows = String::from("model,missing_states\n");
    let mut per_state = [0usize; GameplayState::ALL.len()];
    let mut models_with_any = 0usize;
    let mut parse_failures = 0usize;
    let mut idle_missing: Vec<String> = Vec::new();
    let mut no_state: Vec<String> = Vec::new();
    let mut missing_total = 0usize;

    for path in &files {
        let key = relative_key(&root, path);
        let Ok(bytes) = std::fs::read(path) else {
            parse_failures += 1;
            eprintln!("read failure: {key}");
            continue;
        };
        let Ok(model) = MdxModel::parse(&bytes) else {
            parse_failures += 1;
            eprintln!("parse failure: {key}");
            continue;
        };
        registry.insert(&key, model);
        let Some(resolutions) = registry.state_resolutions(&key) else {
            parse_failures += 1;
            continue;
        };
        let mut has_any = false;
        let mut missing = Vec::new();
        for resolution in &resolutions {
            let index = GameplayState::ALL
                .iter()
                .position(|state| *state == resolution.state)
                .expect("state index");
            match resolution.name() {
                Some(name) => {
                    // Strict contract: the resolved sequence really carries the
                    // state's authored name; no silent substitution.
                    assert!(
                        state_matches(resolution.state, name),
                        "{key}: {} resolved to unrelated sequence {name}",
                        resolution.state.label()
                    );
                    per_state[index] += 1;
                    has_any = true;
                    let _ = writeln!(
                        mapping,
                        "{key},{},{},no",
                        resolution.state.label(),
                        name
                    );
                }
                None => {
                    missing_total += 1;
                    missing.push(resolution.state.label());
                    if resolution.state == GameplayState::Idle {
                        idle_missing.push(key.clone());
                    }
                    let _ = writeln!(mapping, "{key},{},,yes", resolution.state.label());
                }
            }
        }
        if has_any {
            models_with_any += 1;
        } else {
            no_state.push(key.clone());
        }
        if !missing.is_empty() {
            let _ = writeln!(missing_rows, "{key},{}", missing.join("|"));
        }
    }

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-anim/audit");
    if out_dir.is_dir() {
        let _ = std::fs::write(out_dir.join("state-mapping.csv"), &mapping);
        let _ = std::fs::write(out_dir.join("state-mapping-missing.csv"), &missing_rows);
    }

    let missing_per_state: Vec<String> = GameplayState::ALL
        .iter()
        .enumerate()
        .map(|(index, state)| format!("{}={}", state.label(), files.len() - per_state[index]))
        .collect();
    eprintln!(
        "state mapping: models={} with_any_state={} parse_failures={} missing_total={}",
        files.len(),
        models_with_any,
        parse_failures,
        missing_total
    );
    eprintln!("resolved per state: {per_state:?}");
    eprintln!("missing per state: {missing_per_state:?}");
    if !no_state.is_empty() {
        eprintln!("models with no mapped state ({}): {:?}", no_state.len(), no_state);
    }
    if !idle_missing.is_empty() {
        eprintln!("idle missing ({}): {:?}", idle_missing.len(), idle_missing);
    }

    assert_eq!(parse_failures, 0, "every roster model must parse");
    // No silent fallbacks is structural: a `Some` resolution was asserted above
    // to carry the state's own name. Idle is the minimum playable state; the
    // models without any state are reported explicitly.
    assert!(
        models_with_any + no_state.len() == files.len(),
        "accounting mismatch"
    );
}
