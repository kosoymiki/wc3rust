//! Fight-animation contract: a swinging unit must restart its attack clip.
//!
//! Operator: «батраки бьют морлока, хп у него уменьшается, но они не двигаются».
//! The sim was dealing damage while the animator kept the attack clip at its
//! last frame, because `sync_visuals` only replayed a sequence when the *state*
//! changed. This test asserts the observable contract:
//!
//!   * during an `Attack` order the selected sequence name contains "attack";
//!   * the clip time jumps **backwards** at least once (a new swing restarts it);
//!   * the clip time advances between restarts (the clip is actually playing).

use std::path::Path;

use glam::{Vec2, Vec3};
use wc3_game::{Assets, Game};

const MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn justice() -> Option<Game> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(MAP);
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return None;
    }
    // Models (and therefore animations) only exist when the MPQ is mounted;
    // without it the sim still runs but every visual is absent, so the test
    // would pass/fail on the wrong reason.
    let candidates = [
        "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_iso/War3.mpq",
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/com.termux/files/usr/tmp/War3.mpq",
    ];
    let mpq = candidates
        .iter()
        .map(Path::new)
        .find(|path| path.exists())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let assets = if mpq.exists() {
        Assets::load(Some(&mpq), Some(&path))
    } else {
        eprintln!("skip: War3.mpq missing (no models -> no animation to verify)");
        return None;
    };
    let mut game = Game::new(assets, 0x5EED);
    game.enter_battle();
    game.resize(2414.0, 1080.0);
    Some(game)
}

#[test]
fn attacking_unit_restarts_its_attack_clip() {
    let Some(mut game) = justice() else { return };

    // A peon (worker melee) against a neutral creep placed next to it: the
    // operator's exact case.
    let peon = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| (unit.id, unit.pos))
        .expect("worker on Justice");
    let (peon_id, peon_pos) = peon;
    let creep = game
        .sim
        .spawn_def_exact(1, "nban", peon_pos + Vec2::new(70.0, 0.0))
        .expect("creep next to the worker");

    // The attack order goes through the game bridge (same path the HUD uses).
    game.selection = vec![peon_id];
    game.issue(wc3_sim::Command::Attack {
        units: vec![peon_id],
        target: creep,
    });

    let mut last_clip = game.unit_clip_time_ms(peon_id);
    let mut restarts = 0u32;
    let mut advanced = 0u32;
    let mut saw_attack_clip = false;

    for _ in 0..240 {
        game.update(1.0 / 30.0);
        if let Some(name) = game.unit_sequence_name(peon_id) {
            if name.to_ascii_lowercase().contains("attack") {
                saw_attack_clip = true;
            }
        }
        if let Some(now) = game.unit_clip_time_ms(peon_id) {
            match last_clip {
                Some(prev) if now + 1 < prev => restarts += 1,
                Some(prev) if now > prev => advanced += 1,
                _ => {}
            }
            last_clip = Some(now);
        }
    }

    assert!(
        saw_attack_clip,
        "the unit never selected an attack sequence while fighting"
    );
    assert!(
        restarts >= 1,
        "the attack clip never restarted during 240 ticks — a frozen clip is the \
         'they hit but do not move' defect"
    );
    assert!(
        advanced >= 10,
        "the attack clip barely advanced ({advanced} frames) — it is not playing"
    );
}
