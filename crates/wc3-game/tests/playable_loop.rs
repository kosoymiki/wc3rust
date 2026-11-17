//! End-to-end playability on the original 1v1 map: the exact command API a
//! player uses (select → gather → train → build → attack) must produce a
//! real match, not a stub.
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;
use wc3_sim::{Command, Vec2};

#[test]
fn playable_loop_justice() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return;
    }
    // Load the real install: the command card comes from the object tables,
    // exactly as on the device (`War3.mpq` + `War3x.mpq`).
    let mpq = [
        "/data/data/org.wc3rust.p0/files/War3.mpq",
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
    ]
    .iter()
    .map(Path::new)
    .find(|candidate| candidate.is_file());
    let mut game = Game::new(Assets::load(mpq, Some(&path)), 0x5EED);
    eprintln!(
        "0 ASSETS mpq={:?} objects={} models={}",
        mpq.map(|p| p.display().to_string()),
        game.assets.objects.is_some(),
        game.assets.models.len()
    );
    let player = game.player;
    let hall = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == player && unit.def().main_hall)
        .map(|unit| unit.id)
        .expect("player hall");
    let workers: Vec<_> = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .collect();
    assert_eq!(workers.len(), 5, "five starting workers");

    // 1) gather gold with every worker
    let mine = game.sim.nearest_gold_mine(game.sim.unit(workers[0]).unwrap().pos).unwrap();
    game.issue(Command::Harvest { units: workers.clone(), to: mine });
    let gold0 = game.sim.resources[player as usize];
    for _ in 0..2400 { game.update(1.0 / 30.0); }
    let gold1 = game.sim.resources[player as usize];
    eprintln!("1 GATHER gold {gold0} -> {gold1}");
    assert!(gold1 > gold0, "gathering must pay");

    // 2) train the hall's roster: the HUD calls sim.train_def from the card
    game.sim.resources[player as usize] = 5_000;
    game.sim.lumber[player as usize] = 5_000;
    let roster: Vec<String> = game
        .assets
        .train_menu(game.sim.unit(hall).unwrap().def().id)
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    assert!(!roster.is_empty(), "the hall must expose a train menu");
    let mut army0 = game.army_count();
    let mut trained = 0usize;
    for def in roster.iter().take(2) {
        for _ in 0..4 {
            if game.sim.train_def(hall, def.as_str()).is_ok() {
                trained += 1;
            }
        }
        for _ in 0..2_400 { game.update(1.0 / 30.0); }
    }
    eprintln!(
        "2 TRAIN roster={roster:?} queued={trained} army {army0} -> {}",
        game.army_count()
    );
    assert!(trained > 0, "train_def must accept a roster entry");
    assert!(game.army_count() > army0, "training must add a unit");

    // 3) build: place the first *enabled* building that fits near the hall.
    // Enabled now means requirements + gold + lumber (`Sim::can_build_now`),
    // and the site must pass the same `place_denial` predicate the card ghost
    // uses. A large footprint (the hall line) may have no clearing next to a
    // map-authored base, so try the affordable rows in card order.
    // The opening worker may have been killed by the enemy AI during the two
    // training phases; the build step needs a *living* builder.
    let worker = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .next()
        .expect("a living worker for the build step");
    let base = game.sim.unit(hall).unwrap().pos;
    let build_defs: Vec<&'static str> = game
        .sim
        .build_menu_for_owner(player)
        .into_iter()
        .filter(|def| game.sim.can_build_now(player, def.id))
        .map(|def| def.id)
        .collect();
    let mut build_outcome: Option<(&'static str, Result<wc3_sim::UnitId, &'static str>)> = None;
    for def in &build_defs {
        let mut site = None;
        'spot: for ring in 0..24 {
            let radius = 300.0 + ring as f32 * 48.0;
            for step in 0..24 {
                let angle = step as f32 / 24.0 * std::f32::consts::TAU;
                let candidate = base + Vec2::new(angle.cos(), angle.sin()) * radius;
                if game.sim.place_denial(player, def, candidate).is_none() {
                    site = Some(candidate);
                    break 'spot;
                }
            }
        }
        let Some(site) = site else { continue };
        let placed = game.sim.place_building_ordered(worker, def, site);
        let ok = placed.is_ok();
        build_outcome = Some((def, placed));
        if ok {
            break;
        }
    }
    if let Some((def, placed)) = &build_outcome {
        for _ in 0..2_000 { game.update(1.0 / 30.0); }
        eprintln!("3 BUILD {def} placed={:?} buildings={}", placed.is_ok(), game.sim.alive_units().filter(|u| u.def().stats.is_building).count());
    }
    assert!(
        build_outcome.as_ref().map(|(_, placed)| placed.is_ok()).unwrap_or(false),
        "building placement must work for at least one enabled row: {build_defs:?} -> {build_outcome:?}"
    );

    // 4) attack the enemy base with everything and let the AI answer
    army0 = game.army_count();
    let enemy0 = game.enemy_count();
    let enemy_base = game.enemy_base;
    let units: Vec<_> = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == player && !unit.is_building())
        .map(|unit| unit.id)
        .collect();
    if !units.is_empty() {
        game.issue(Command::MoveAttack { units, to: enemy_base });
    }
    let mut kills0 = game.total_kills;
    for _ in 0..18_000 {
        game.update(1.0 / 30.0);
        if game.phase != wc3_game::game::Phase::Playing { break; }
    }
    let kills1 = game.total_kills;
    eprintln!(
        "4 FIGHT kills {kills0} -> {kills1} phase={:?} army {army0} -> {} enemy {enemy0} -> {}",
        game.phase,
        game.army_count(),
        game.enemy_count()
    );
    let combat =
        kills1 > kills0 || game.enemy_count() < enemy0 || game.army_count() < army0;
    assert!(
        combat || game.phase != wc3_game::game::Phase::Playing,
        "a 10-minute attack must produce combat, not a frozen board"
    );
}
