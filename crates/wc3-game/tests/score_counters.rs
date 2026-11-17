//! `game.score.counters` — the WC3 score screen rows are computed from real
//! sim events: deposits (gold/lumber after upkeep), trained units, finished
//! buildings, kills/losses, razed buildings, hero levels and local APM.
use glam::Vec2;
use wc3_game::game::Game;
use wc3_game::Assets;
use wc3_sim::{Command, Event, UnitKind};

fn game() -> Game {
    let mut game = Game::new(Assets::load(None, None), 0x469);
    game.enter_battle();
    game.resize(1280.0, 720.0);
    game
}

#[test]
fn game_score_counters() {
    let mut game = game();
    let player = game.player;
    let enemy = game.enemy;

    // --- income: the sim's deposit API is the script/economy income path.
    let income = game.sim.upkeep(player).income_permille().max(1);
    let gold_paid = ((120 * income / 1000).max(1)) as u32;
    let lumber_paid = ((40 * income / 1000).max(1)) as u32;
    game.sim.deposit_gold(player, 120);
    game.sim.deposit_lumber(player, 40);
    game.drain_sim_events();
    let row = game.score_rows()[0].clone();
    assert_eq!(row.seat, player);
    assert_eq!(row.gold_gathered, 120, "score counts gross gold mined");
    assert_eq!(row.upkeep_paid, 120u32 - gold_paid, "upkeep is the withheld tax");
    assert_eq!(row.lumber_gathered, lumber_paid);

    // --- training and construction are event-driven counters.
    let hall = game.player_hall().expect("player hall");
    game.sim.events.push(Event::BuildingComplete(hall));
    let recruit = game
        .sim
        .spawn(player, UnitKind::Footman, game.base_pos + Vec2::new(180.0, 60.0));
    game.sim.events.push(Event::UnitTrained(recruit));
    game.drain_sim_events();
    let row = game.score_rows()[0].clone();
    assert_eq!(row.buildings_built, 1);
    assert_eq!(row.units_trained, 1);

    // --- deaths: own losses, enemy kills, and a razed enemy building.
    // The victims are chosen so no seat loses its last building *and* worker
    // (that would end the match and stop the APM clock below).
    let find = |owner: u8, building: bool, worker: Option<bool>| {
        game.sim
            .units
            .iter()
            .find(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building() == building
                    && worker.is_none_or(|want| unit.def().stats.is_worker == want)
            })
            .map(|unit| unit.id)
    };
    let enemy_soldier = find(enemy, false, Some(false)).expect("enemy soldier");
    let player_soldier = find(player, false, Some(false)).expect("player soldier");
    let enemy_building = find(enemy, true, None).expect("enemy building");
    game.sim.kill_unit(enemy_soldier);
    game.sim.kill_unit(player_soldier);
    game.sim.kill_unit(enemy_building);
    game.drain_sim_events();
    assert_eq!(
        game.phase,
        wc3_game::game::Phase::Playing,
        "the score sample must not end the match"
    );

    let rows = game.score_rows();
    let (local, rival) = (&rows[0], &rows[1]);
    assert_eq!(local.units_killed, 1, "one enemy unit killed");
    assert_eq!(local.buildings_razed, 1, "one enemy building razed");
    assert_eq!(local.units_lost, 1, "one own unit lost");
    assert_eq!(rival.units_lost, 1, "enemy lost one unit");
    assert_eq!(rival.units_killed, 1, "enemy killed one unit");
    assert_eq!(rival.buildings_razed, 0, "the player lost no building");
    assert!(
        local.hero_level >= 1,
        "the demo seats a hero; level {}",
        local.hero_level
    );

    // --- APM: orders issued through the game layer per simulated minute.
    let soldier = game
        .sim
        .units
        .iter()
        .find(|unit| unit.alive && unit.owner == player && !unit.is_building())
        .map(|unit| unit.id)
        .expect("player unit");
    game.issue(Command::Stop {
        units: vec![soldier],
    });
    for _ in 0..40 {
        game.update(0.034);
    }
    assert!(
        game.score_rows()[0].apm >= 1,
        "a queued order must raise APM, got {}",
        game.score_rows()[0].apm
    );

    let line = game.score_line();
    assert!(line.starts_with("GAME_SCORE"), "line: {line}");
    assert!(line.contains(&format!("s{player}[")), "line: {line}");
    assert!(line.contains(&format!("s{enemy}[")), "line: {line}");

    // The HUD score screen consumes the same numbers as label/value rows.
    let data = game.score_data();
    assert_eq!(data.rows.len(), 10, "ten WC3 score fields");
    assert_eq!(data.title, "SCORE");
    assert!(
        data.rows.iter().any(|row| row.label == "UNITS KILLED"),
        "rows: {:?}",
        data.rows.iter().map(|row| &row.label).collect::<Vec<_>>()
    );
}
