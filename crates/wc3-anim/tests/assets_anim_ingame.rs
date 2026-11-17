//! `assets.anim.ingame`: the animation runtime selects the authored sequence
//! for each gameplay state (idle/walk/attack/death/cast/channel), reports a
//! missing state instead of silently playing another sequence, and state
//! changes restart the clock inside the new sequence's own time range.
//!
//! This is a unit test on the anim runtime (not a screenshot test): it mirrors
//! what the game engine does per visual update.

use std::path::{Path, PathBuf};

use wc3_anim::{
    ActivityOrder, AnimatorRegistry, GameplayState, UnitActivity, hover_uses_idle, select_state,
};
use wc3_formats::mdx::MdxModel;

fn assets_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["analysis/war3_full", "analysis/war3_samples"]
        .iter()
        .map(|tree| root.join(tree))
        .find(|candidate| candidate.join("Units/UnitData.slk").is_file())
}

fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|file| file.to_str())
            .map(|file| file.eq_ignore_ascii_case(name))
            == Some(true)
        {
            return Some(path);
        }
    }
    None
}

fn model_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for segment in relative.split('/') {
        if current.is_dir() {
            if let Some(found) = find_case_insensitive(&current, segment) {
                current = found;
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    current.is_file().then_some(current)
}

fn load(
    registry: &mut AnimatorRegistry,
    root: &Path,
    relative: &str,
) -> Option<(String, MdxModel)> {
    let path = model_path(root, relative)?;
    let bytes = std::fs::read(&path).ok()?;
    let model = MdxModel::parse(&bytes).ok()?;
    let key = relative.to_string();
    registry.insert(&key, model.clone());
    Some((key, model))
}

/// Drive one animator through a state for 25% of its authored duration.
fn drive(animator: &mut wc3_anim::Animator, start: u32, end: u32) -> u32 {
    let step = ((end - start).max(1) as f32) * 0.25 / 1000.0;
    animator.update(step);
    let time = animator.frame_time();
    assert!(
        time >= start && time <= end,
        "frame time {time} escaped sequence range {start}..{end}"
    );
    for matrix in animator.world_matrices() {
        assert!(matrix.to_cols_array().iter().all(|value| value.is_finite()));
    }
    time
}

#[test]
fn assets_anim_ingame_engine_state_selection() {
    let Some(root) = assets_root() else {
        eprintln!("skipping: shipped model tree missing");
        return;
    };
    let mut registry = AnimatorRegistry::new(8);

    // (model, required states: name predicate)
    struct Case {
        path: &'static str,
        required: &'static [(GameplayState, fn(&str) -> bool)],
        absent: &'static [GameplayState],
    }
    fn contains(needle: &'static str) -> impl Fn(&str) -> bool {
        move |name| name.to_ascii_lowercase().contains(needle)
    }
    let cases = [
        Case {
            path: "Units/Human/Footman/Footman.mdx",
            required: &[
                (GameplayState::Idle, |name| contains("stand")(name)),
                (GameplayState::Walk, |name| contains("walk")(name)),
                (GameplayState::Attack, |name| contains("attack")(name)),
                (GameplayState::Death, |name| contains("death")(name)),
            ],
            absent: &[GameplayState::Cast, GameplayState::Channel],
        },
        Case {
            path: "Units/NightElf/Archer/Archer.mdx",
            required: &[
                (GameplayState::Idle, |name| contains("stand")(name)),
                (GameplayState::Walk, |name| contains("walk")(name)),
                (GameplayState::Attack, |name| contains("attack")(name)),
                (GameplayState::Death, |name| contains("death")(name)),
            ],
            absent: &[GameplayState::Cast, GameplayState::Channel],
        },
        Case {
            path: "Units/Human/HeroArchMage/HeroArchMage.mdx",
            required: &[
                (GameplayState::Idle, |name| contains("stand")(name)),
                (GameplayState::Walk, |name| contains("walk")(name)),
                (GameplayState::Attack, |name| contains("attack")(name)),
                (GameplayState::Death, |name| contains("death")(name)),
                (GameplayState::Cast, |name| name == "Spell"),
                (GameplayState::Channel, |name| name == "Stand Channel"),
            ],
            absent: &[],
        },
        Case {
            path: "Units/Orc/HeroBladeMaster/HeroBladeMaster.mdx",
            required: &[
                (GameplayState::Cast, |name| name == "Attack Walk Stand Spin"),
                (GameplayState::Death, |name| contains("death")(name)),
            ],
            absent: &[GameplayState::Channel],
        },
        Case {
            path: "Units/Undead/HeroDeathKnight/HeroDeathKnight.mdx",
            required: &[(GameplayState::Cast, |name| contains("spell")(name))],
            absent: &[GameplayState::Channel],
        },
        Case {
            path: "Buildings/Human/TownHall/TownHall.mdx",
            required: &[
                (GameplayState::Idle, |name| contains("stand")(name)),
                (GameplayState::Death, |name| contains("death")(name)),
                (GameplayState::Birth, |name| contains("birth")(name)),
                (GameplayState::Build, |name| contains("stand work")(name)),
            ],
            absent: &[GameplayState::Walk, GameplayState::Attack],
        },
        Case {
            path: "Units/Human/Peasant/peasant.mdx",
            required: &[
                (GameplayState::Idle, |name| contains("stand")(name)),
                (GameplayState::Walk, |name| contains("walk")(name)),
                (GameplayState::Attack, |name| contains("attack")(name)),
                (GameplayState::Death, |name| contains("death")(name)),
                (GameplayState::Build, |name| contains("stand work")(name)),
            ],
            absent: &[GameplayState::Cast, GameplayState::Channel, GameplayState::Birth],
        },
        Case {
            path: "Abilities/Weapons/FireBallMissile/FireBallMissile.mdx",
            required: &[
                (GameplayState::Idle, |name| name == "Stand"),
                (GameplayState::Death, |name| name == "Death"),
            ],
            absent: &[
                GameplayState::Walk,
                GameplayState::Attack,
                GameplayState::Cast,
                GameplayState::Channel,
            ],
        },
    ];

    let mut checked = 0usize;
    for case in &cases {
        let Some((key, model)) = load(&mut registry, &root, case.path) else {
            panic!("fixture missing: {}", case.path);
        };
        let resolutions = registry.state_resolutions(&key).expect("resolutions");
        for (state, predicate) in case.required {
            let resolution = resolutions
                .iter()
                .find(|resolution| resolution.state == *state)
                .expect("state row");
            let name = resolution.name().unwrap_or_else(|| {
                panic!("{} must author {} but it is reported missing", key, state.label())
            });
            assert!(
                predicate(name),
                "{}: {} resolved to {name}, which does not match the state",
                key,
                state.label()
            );
            let (matched, mut animator) = registry
                .state_animator(&key, *state)
                .unwrap_or_else(|| panic!("{key}: no animator for {}", state.label()));
            drive(
                &mut animator,
                matched.sequence.start,
                matched.sequence.end,
            );
            checked += 1;
        }
        for state in case.absent {
            let resolution = resolutions
                .iter()
                .find(|resolution| resolution.state == *state)
                .expect("state row");
            assert!(
                resolution.missing(),
                "{}: {} must be reported missing, got {:?}",
                key,
                state.label(),
                resolution.name()
            );
            assert!(
                registry.state_animator(&key, *state).is_none(),
                "{}: missing {} must not produce an animator (silent fallback)",
                key,
                state.label()
            );
        }
        // The model really is the parsed MDX with the sequences we attest.
        assert!(!model.sequences.is_empty());
    }

    // State switching restarts the clock inside the new sequence.
    let (footman_key, _) = load(&mut registry, &root, "Units/Human/Footman/Footman.mdx")
        .expect("Footman fixture");
    let (idle, mut animator) = registry
        .state_animator(&footman_key, GameplayState::Idle)
        .expect("idle");
    drive(&mut animator, idle.sequence.start, idle.sequence.end);
    for state in [
        GameplayState::Walk,
        GameplayState::Attack,
        GameplayState::Death,
        GameplayState::Idle,
    ] {
        let (matched, mut next) = registry
            .state_animator(&footman_key, state)
            .unwrap_or_else(|| panic!("no animator for {}", state.label()));
        assert_eq!(
            next.frame_time(),
            matched.sequence.start,
            "state switch must restart at the sequence start"
        );
        // Old animator is independent; the new one advances only in its range.
        drive(&mut next, matched.sequence.start, matched.sequence.end);
    }

    eprintln!("ingame state selection: {checked} state playbacks verified across representative models");
    assert!(checked >= 20, "expected broad state coverage, got {checked}");
}


/// `assets.anim.ingame`: corrected activity priority. Walking must not freeze in
/// the attack pose, construction must play Birth, and buildings/workers/casters
/// map to their own state family.
#[test]
fn assets_anim_ingame_state_priority_matrix() {
    let base = UnitActivity::default();
    let cases = [
        (
            "dead while walking",
            UnitActivity { dead: true, path_empty: false, ..base },
            GameplayState::Death,
        ),
        (
            "building under construction",
            UnitActivity { building: true, constructing: true, ..base },
            GameplayState::Birth,
        ),
        (
            "walking with cooldown running",
            UnitActivity { cooldown: true, path_empty: false, ..base },
            GameplayState::Walk,
        ),
        (
            "standing attack order",
            UnitActivity { order: ActivityOrder::Attack, path_empty: true, ..base },
            GameplayState::Attack,
        ),
        (
            "worker building",
            UnitActivity { order: ActivityOrder::Build, path_empty: true, ..base },
            GameplayState::Build,
        ),
        (
            "building idle",
            UnitActivity { building: true, ..base },
            GameplayState::Idle,
        ),
        (
            "tower attacking",
            UnitActivity { building: true, order: ActivityOrder::Attack, path_empty: true, ..base },
            GameplayState::Attack,
        ),
        (
            "hero casting",
            UnitActivity { casting: true, ..base },
            GameplayState::Cast,
        ),
        (
            "hero channeling",
            UnitActivity { channeling: true, ..base },
            GameplayState::Channel,
        ),
        (
            "casting while walking",
            UnitActivity { casting: true, path_empty: false, ..base },
            GameplayState::Cast,
        ),
        (
            "hovering worker moving",
            UnitActivity { hover: true, path_empty: false, ..base },
            GameplayState::Walk,
        ),
    ];
    for (label, activity, expected) in cases {
        assert_eq!(select_state(&activity), expected, "{label}");
    }
    // State priority is exactly the engine contract; no path can reach a
    // silent Attack-while-Walk combination.
    assert_ne!(
        select_state(&UnitActivity { cooldown: true, path_empty: false, ..base }),
        GameplayState::Attack
    );
    // Wisp: no Walk authored -> documented hover substitution, never a silent
    // fallback inside the resolver.
    let hovering = UnitActivity { hover: true, path_empty: false, ..base };
    assert!(hover_uses_idle(&hovering), "hovering movement uses the idle pose");
    assert!(!hover_uses_idle(&UnitActivity { path_empty: false, ..base }));
    eprintln!("ingame state priority: {} cases verified", cases.len());
}
