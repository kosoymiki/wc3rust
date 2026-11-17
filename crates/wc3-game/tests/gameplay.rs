//! End-to-end game wiring: selection through commands, combat, production and
//! win/lose. These run without a GPU and catch regressions in the layer between
//! the sim and the HUD.
use glam::Vec2;
use wc3_game::{Assets, Game, Phase};
use wc3_sim::{Command, UnitKind};

fn game() -> Game {
    let mut game = Game::new(Assets::load(None, None), 0xABCD);
    game.resize(1280.0, 720.0);
    game
}

fn own_unit(game: &Game, kind: UnitKind) -> wc3_sim::UnitId {
    game.sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.kind == kind)
        .map(|unit| unit.id)
        .expect("starting unit")
}

#[test]
fn move_command_moves_the_selected_footman() {
    let mut game = game();
    let footman = own_unit(&game, UnitKind::Footman);
    let start = game.sim.unit(footman).unwrap().pos;
    let target = start + Vec2::new(900.0, 400.0);
    game.issue(Command::Move { units: vec![footman], to: target });
    for _ in 0..90 {
        game.update(1.0 / 30.0);
    }
    let moved = game.sim.unit(footman).unwrap().pos.distance(start);
    assert!(moved > 200.0, "footman should move, moved {moved:.1}");
}

#[test]
fn attack_order_damages_the_enemy_squad() {
    let mut game = game();
    let footman = own_unit(&game, UnitKind::Footman);
    game.issue(Command::MoveAttack {
        units: vec![footman],
        to: game.enemy_base,
    });
    let enemies: Vec<_> = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == game.enemy && !unit.is_building())
        .map(|unit| unit.id)
        .collect();
    let hp_before: i32 = enemies
        .iter()
        .map(|id| game.sim.unit(*id).unwrap().hp)
        .sum();
    for _ in 0..900 {
        game.update(1.0 / 30.0);
    }
    let hp_after: i32 = enemies
        .iter()
        .filter_map(|id| game.sim.unit(*id))
        .map(|unit| unit.hp)
        .sum();
    assert!(hp_after < hp_before, "combat must connect: {hp_before} -> {hp_after}");
}

#[test]
fn training_a_footman_increases_the_army() {
    let mut game = game();
    let hall = game.player_hall().unwrap();
    game.selection = vec![hall];
    game.activate_action(wc3_game::UiAction::Train(UnitKind::Footman));
    let before = game.army_count();
    for _ in 0..800 {
        game.update(1.0 / 30.0);
    }
    assert!(game.army_count() > before, "army {} -> {}", before, game.army_count());
    assert!(game.total_trained >= 1);
}

#[test]
fn destroying_the_enemy_hall_alone_does_not_win_the_match() {
    // WC3 base rule: a seat is out when it has no building left *and* no worker
    // to rebuild one. `game_match_victory` covers the full wipe; a dead hall
    // with a barracks and workers still standing is not a victory.
    let mut game = game();
    for unit in game.sim.units.iter_mut() {
        if unit.owner == game.enemy && unit.kind == UnitKind::TownHall {
            unit.alive = false;
        }
    }
    game.update(1.0 / 30.0);
    assert_eq!(game.phase, Phase::Playing);
    assert!(game.match_outcome().is_none());
}
