//! Typed `wc3_re::gameplay::GameplayConfig` consumption in the sim.

use std::path::Path;

use wc3_sim::{AttackType, GameplayConfig, Sim, Vec2};

#[test]
fn loads_typed_constants_from_the_install() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GameplayConfig::load(&root).expect("MiscGame.txt gameplay constants");
    assert_eq!(config.max_hero_level, 10);
    assert_eq!(config.need_hero_xp.first().copied(), Some(200));
    assert_eq!(config.grant_hero_xp.first().copied(), Some(80));
    assert_eq!(config.str_hit_point_bonus, 25);
    assert_eq!(config.int_mana_bonus, 15);
    assert_eq!(config.hero_exp_range, 1200.0);
    let mut sim = Sim::new(70, 32, 32);
    sim.apply_gameplay(config.clone());
    assert_eq!(sim.gameplay_config().need_hero_xp, config.need_hero_xp);
    assert_eq!(sim.gameplay_config().str_hit_point_bonus, 25);
}

#[test]
fn custom_constants_drive_leveling_speed_and_kill_xp() {
    let mut config = GameplayConfig::default();
    config.need_hero_xp[0] = 1;
    config.str_hit_point_bonus = 50;
    config.max_unit_speed = 100.0;
    config.grant_hero_xp = vec![777; 10];
    config.hero_exp_range = 200.0;
    let mut sim = Sim::new(71, 32, 32);
    sim.passive_income = false;
    sim.apply_gameplay(config);
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(400.0, 400.0))
        .expect("paladin");
    let footman = sim
        .spawn_def(0, "hfoo", Vec2::new(480.0, 400.0))
        .expect("footman");
    assert_eq!(sim.unit(hero).unwrap().speed, 100.0, "speed clamped");
    assert_eq!(sim.unit(footman).unwrap().speed, 100.0);
    let hp_before = sim.unit(hero).unwrap().max_hp;
    assert_eq!(sim.add_hero_xp(hero, 1).expect("level"), 2);
    assert_eq!(
        sim.unit(hero).unwrap().max_hp,
        hp_before + 150,
        "STR gain rounds 2.7 -> 3, 50 HP each"
    );
    let creep = sim
        .spawn_def(2, "nftr", Vec2::new(410.0, 400.0))
        .expect("creep");
    let _ = sim.apply_damage(hero, creep, 10_000, AttackType::Magic);
    assert_eq!(
        sim.hero_xp(hero),
        Some(1 + 777),
        "level-2 kill grants configured XP"
    );
}
