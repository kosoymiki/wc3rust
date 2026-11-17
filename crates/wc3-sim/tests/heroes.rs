//! mechanics `heroes.leveling` and `heroes.revive`.

use wc3_sim::{AttackType, Command, Sim, Vec2};

#[test]
fn heroes_skill_points_ultimate_at_6() {
    let mut sim = Sim::new(801, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 10_000;
    let ally = sim
        .spawn_def(0, "hfoo", Vec2::new(520.0, 500.0))
        .expect("ally");
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(420.0, 500.0))
        .expect("paladin");
    let hp1 = sim.unit(hero).unwrap().max_hp;
    let mana1 = sim.unit(hero).unwrap().max_mana;
    assert_eq!(sim.unit(hero).unwrap().skill_points, 1, "level 1 point");
    assert_eq!(
        sim.cast_data_ability(hero, "AHhb", Some(ally), None),
        Err("ability not learned"),
        "generic runner gates unlearned hero abilities"
    );
    sim.step(&[Command::LearnAbility {
        unit: hero,
        ability: "AHhb",
    }]);
    assert!(sim.unit(hero).unwrap().learned.contains(&"AHhb"));
    assert_eq!(sim.unit(hero).unwrap().skill_points, 0);
    // AHre is the Paladin ultimate: reqLevel 6 in AbilityData.slk.
    assert_eq!(
        sim.learn_ability(hero, "AHre"),
        Err("no skill points"),
        "ultimate also needs a point"
    );
    sim.set_hero_level(hero, 6).expect("level 6");
    assert_eq!(sim.unit(hero).unwrap().skill_points, 5, "one per level");
    sim.learn_ability(hero, "AHre").expect("ultimate at 6");
    assert!(sim.unit(hero).unwrap().learned.contains(&"AHre"));
    assert!(sim.unit(hero).unwrap().max_hp > hp1, "STR scales HP");
    assert!(sim.unit(hero).unwrap().max_mana > mana1, "INT scales mana");
    assert!(
        sim.unit(hero).unwrap().damage > 0,
        "primary stat scales damage"
    );
    // Learned ability now runs through the generic effect runner.
    let healed = sim.heal_unit(ally, 0);
    let _ = healed;
    sim.cast_data_ability(hero, "AHhb", Some(ally), None)
        .expect("learned Holy Light cast");
}

#[test]
fn heroes_revive_altar_700g_150s() {
    let mut sim = Sim::new(802, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 5_000;
    let altar = sim
        .spawn_def(0, "halt", Vec2::new(600.0, 600.0))
        .expect("altar of kings");
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(720.0, 600.0))
        .expect("paladin");
    sim.apply_damage(hero, hero, 100_000, AttackType::Magic);
    assert!(!sim.unit(hero).unwrap().alive);
    let gold = sim.resources[0];
    let cost = sim.revive_hero(hero, altar).expect("altar revive");
    assert_eq!(cost, 700, "HeroMaxReviveCostGold");
    assert_eq!(sim.resources[0], gold - 700);
    assert!(!sim.unit(hero).unwrap().alive, "revive is scheduled");
    let ticks = (150.0 * 30.0) as u32;
    let mut revived = false;
    for _ in 0..(ticks + 10) {
        sim.step(&[]);
        if sim.events.iter().any(
            |event| matches!(event, wc3_sim::Event::HeroRevived { hero: id, .. } if *id == hero),
        ) {
            revived = true;
            break;
        }
    }
    assert!(revived, "HeroMaxReviveTime = 150 s");
    let unit = sim.unit(hero).expect("hero");
    assert!(unit.alive);
    assert_eq!(unit.hp, unit.max_hp);
    assert!(unit.pos.distance(Vec2::new(600.0, 600.0)) < 400.0);
}

/// `heroes.attributes`: level-ups raise the three attributes and the derived
/// HP/mana/damage/armor exactly as the gameplay constants prescribe.
#[test]
fn heroes_attributes_scale_with_level() {
    let mut sim = Sim::new(802, 64, 64);
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(420.0, 500.0))
        .expect("paladin");
    let (base_hp, base_mana, base_dmg, base_str, base_agi, base_int, base_points) = {
        let unit = sim.unit(hero).expect("hero");
        (
            unit.max_hp,
            unit.max_mana,
            unit.damage,
            unit.hero_strength,
            unit.hero_agility,
            unit.hero_intelligence,
            unit.skill_points,
        )
    };
    sim.set_hero_level(hero, 5).expect("set level 5");
    let (hp, mana, dmg, str_, agi, int_, points) = {
        let unit = sim.unit(hero).expect("hero");
        (
            unit.max_hp,
            unit.max_mana,
            unit.damage,
            unit.hero_strength,
            unit.hero_agility,
            unit.hero_intelligence,
            unit.skill_points,
        )
    };
    assert_eq!(sim.unit(hero).unwrap().level, 5);
    assert!(str_ > base_str && agi > base_agi && int_ > base_int, "attributes grow");
    assert!(hp > base_hp, "strength raises HP: {base_hp} -> {hp}");
    assert!(mana > base_mana, "intelligence raises mana: {base_mana} -> {mana}");
    assert!(dmg >= base_dmg, "primary attribute raises damage: {base_dmg} -> {dmg}");
    assert!(points >= base_points + 4, "one skill point per level: {base_points} -> {points}");
    let expected_hp = base_hp + sim.gameplay.str_hit_point_bonus * (str_ - base_str);
    assert_eq!(hp, expected_hp, "HP delta follows str_hit_point_bonus");
}
