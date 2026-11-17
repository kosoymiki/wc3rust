//! Merge-gate tests for the execution surface added for the game/JASS bridge:
//! capability-driven hall/supply identity, remote construction orders,
//! script-facing unit mutations and AI wave events.

mod common;

use common::*;
use wc3_sim::data::{Race, def_of, supply_building_for};
use wc3_sim::{Event, Order, OrderSpec, Sim, Spell, UnitDefId, UnitKind, Vec2, ai::hall_for_race};

#[test]
fn main_hall_and_supply_building_are_data_driven() {
    for race in [Race::Human, Race::Orc, Race::Undead, Race::NightElf] {
        let hall = def_of(hall_for_race(race)).expect("hall def");
        assert!(hall.is_main_hall(), "{} hall", race.name());
        let supply = supply_building_for(race).expect("supply building");
        assert!(
            !supply.is_main_hall(),
            "{} supply is not a hall",
            race.name()
        );
        assert!(
            supply.stats.supply_provided > 0,
            "{} supply building provides food",
            race.name()
        );
        assert!(supply.stats.cost > 0, "{} supply cost", race.name());
    }
    // Human data anchor: farm, not the town hall.
    assert_eq!(supply_building_for(Race::Human).unwrap().id, "hhou");
    assert_eq!(def_of("htow").unwrap().main_hall, true);
    assert_eq!(def_of("hhou").unwrap().main_hall, false);
}

#[test]
fn place_building_ordered_walks_a_far_worker_and_completes() {
    let mut sim = Sim::new(201, 64, 64);
    sim.passive_income = false;
    sim.resources = [100_000, 100_000];
    let hall_id = hall(&mut sim, 0, Vec2::new(500.0, 500.0));
    let worker = worker(&mut sim, 0, Vec2::new(5600.0, 500.0));
    sim.step(&[]);
    let site = sim
        .place_building_ordered(worker, "hhou", Vec2::new(1500.0, 500.0))
        .expect("remote build order");
    assert!(sim.unit(site).unwrap().construct.is_some());
    assert!(
        sim.unit(worker)
            .unwrap()
            .pos
            .distance(Vec2::new(1500.0, 500.0))
            > 420.0,
        "worker starts far from the site"
    );
    let mut completed = false;
    for _ in 0..3000 {
        sim.step(&[]);
        if sim
            .unit(site)
            .map(|unit| unit.construct.is_none() && unit.hp == unit.max_hp)
            .unwrap_or(false)
        {
            completed = true;
            break;
        }
    }
    assert!(completed, "far worker walked and finished the farm");
    assert!(
        sim.supply_cap(0) >= sim.unit(hall_id).unwrap().def().stats.supply_provided + 6,
        "farm raised the supply cap"
    );
}

#[test]
fn legacy_place_building_still_requires_a_nearby_worker() {
    let mut sim = Sim::new(202, 64, 64);
    sim.resources = [100_000, 100_000];
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let far = worker(&mut sim, 0, Vec2::new(5000.0, 5000.0));
    assert_eq!(
        sim.place_building(far, "hhou", Vec2::new(400.0, 1000.0)),
        Err("builder too far")
    );
}

#[test]
fn script_mutations_clamp_and_kill_through_public_api() {
    let mut sim = Sim::new(203, 32, 32);
    let unit = footman(&mut sim, 0, Vec2::new(500.0, 500.0));
    sim.set_unit_hp(unit, 100);
    assert_eq!(sim.unit(unit).unwrap().hp, 100);
    sim.set_unit_hp(unit, 99_999);
    assert_eq!(sim.unit(unit).unwrap().hp, sim.unit(unit).unwrap().max_hp);

    let castle = sim.spawn_def(0, "hcas", Vec2::new(1500.0, 500.0)).unwrap();
    sim.step(&[]);
    let cell = sim.grid.world_to_cell(Vec2::new(1500.0, 500.0));
    assert!(!sim.grid.walkable(cell.0, cell.1));
    // Teleporting clears any path.
    sim.issue(&[unit], OrderSpec::Move(Vec2::new(2500.0, 500.0)));
    assert!(!sim.unit(unit).unwrap().path.is_empty());
    sim.teleport_unit(unit, Vec2::new(700.0, 900.0));
    assert_eq!(sim.unit(unit).unwrap().pos, Vec2::new(700.0, 900.0));
    assert!(sim.unit(unit).unwrap().path.is_empty());
    // Killing a building frees its footprint and emits a death event.
    sim.kill_unit(castle);
    assert!(!sim.unit(castle).unwrap().alive);
    assert!(sim.grid.walkable(cell.0, cell.1));
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::UnitDied(id) if *id == castle))
    );

    let paladin = sim.spawn_def(0, "Hpal", Vec2::new(300.0, 300.0)).unwrap();
    let max_mana = sim.unit(paladin).unwrap().max_mana;
    sim.set_unit_mana(paladin, 99_999);
    assert_eq!(sim.unit(paladin).unwrap().mana, max_mana);
    sim.set_unit_mana(paladin, -5);
    assert_eq!(sim.unit(paladin).unwrap().mana, 0);
    assert!(matches!(sim.unit(paladin).unwrap().order, Order::Idle));
    let _ = Spell::HolyLight;
}

#[test]
fn ai_wave_event_matches_wave_counter() {
    let mut sim = Sim::new(204, 64, 64);
    sim.passive_income = false;
    sim.resources = [20_000, 20_000];
    hall(&mut sim, 0, Vec2::new(7000.0, 7000.0));
    hall(&mut sim, 1, Vec2::new(900.0, 900.0));
    sim.add_gold_mine(Vec2::new(1500.0, 900.0), 20_000);
    worker(&mut sim, 1, Vec2::new(1100.0, 1000.0));
    sim.step(&[]);
    sim.enable_ai(
        1,
        wc3_sim::AiConfig {
            workers: 2,
            army_supply: 2,
            first_attack_tick: 60,
            wave_interval: 600,
            retreat_permille: 300,
            composition: &[("hfoo", 1)],
            worker: "hpea",
        },
    );
    let mut wave_events = Vec::new();
    for _ in 0..6000 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::AiWave { owner, wave } = event {
                wave_events.push((*owner, *wave));
            }
        }
        if sim.ai_waves(1) >= 1 && !wave_events.is_empty() {
            break;
        }
    }
    assert_eq!(wave_events.first(), Some(&(1, 1)));
    assert_eq!(
        wave_events.iter().filter(|(_, wave)| *wave == 1).count(),
        1,
        "one event per wave: {wave_events:?}"
    );
    assert!(sim.ai_waves(1) >= 1);
}

#[test]
fn upkeep_tiers_are_shared_with_deposits() {
    let mut sim = Sim::new(205, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    assert_eq!(sim.upkeep_permille(0), 1000);
    // 26 footmen = 52 food.
    for i in 0..26 {
        footman(&mut sim, 0, Vec2::new(200.0 + (i % 5) as f32 * 40.0, 900.0));
    }
    sim.step(&[]);
    assert_eq!(sim.supply_used(0), 52);
    assert_eq!(sim.upkeep_permille(0), 700);
}

#[test]
fn legacy_build_command_charges_the_hall_once_and_rejects_when_broke() {
    let mut sim = Sim::new(206, 32, 32);
    sim.passive_income = false;
    let builder = worker(&mut sim, 0, Vec2::new(400.0, 400.0));
    let before_units = sim.alive_units().count();
    sim.resources[0] = 1000;
    sim.step(&[wc3_sim::Command::Build {
        builder,
        kind: wc3_sim::UnitKind::TownHall,
        at: Vec2::new(900.0, 400.0),
    }]);
    assert_eq!(sim.resources[0], 550, "town hall costs 450 once");
    assert_eq!(sim.alive_units().count(), before_units + 1);

    // A second hall with only 400 gold must be rejected with no charge/spawn
    // (450 is the real WC3 hall cost; 500 would legitimately be affordable).
    let mid_units = sim.alive_units().count();
    sim.resources[0] = 400;
    sim.step(&[wc3_sim::Command::Build {
        builder,
        kind: wc3_sim::UnitKind::TownHall,
        at: Vec2::new(1400.0, 400.0),
    }]);
    assert_eq!(sim.resources[0], 400, "rejected build refunds nothing");
    assert_eq!(sim.alive_units().count(), mid_units);
    assert!(
        sim.events.iter().any(
            |event| matches!(event, wc3_sim::Event::Error(text) if *text == "not enough gold")
        )
    );
}

#[test]
fn build_menu_is_derived_from_the_generated_slk_roster() {
    for race in [Race::Human, Race::Orc, Race::Undead, Race::NightElf] {
        let menu = wc3_sim::data::build_menu(race);
        assert!(menu.len() >= 6, "{} buildings: {}", race.name(), menu.len());
        let supply = supply_building_for(race).unwrap();
        assert_eq!(menu[0].id, supply.id, "{} supply first", race.name());
        assert!(
            menu.iter().any(|def| def.is_main_hall()),
            "{} hall present",
            race.name()
        );
        // Only worker-buildable towers can appear in the build menu; the
        // Undead Spirit Tower (uzg1) is a Ziggurat upgrade, not a build entry,
        // and the Human Guard/Cannon towers are Scout Tower upgrades.
        let has_defence = wc3_sim::ROSTER.iter().any(|def| {
            def.race == race
                && def.is_building()
                && wc3_sim::data::is_worker_buildable(def)
                && !def.is_main_hall()
                && def.stats.damage > 0
        });
        if has_defence {
            assert!(
                menu.iter()
                    .any(|def| !def.is_main_hall() && def.stats.damage > 0),
                "{} defence present",
                race.name()
            );
        }
        assert!(
            menu.iter()
                .any(|def| !def.is_main_hall() && def.trains_units()),
            "{} producer present",
            race.name()
        );
        for def in &menu {
            assert_eq!(def.race, race);
            assert!(def.is_building());
            assert!(def.stats.cost > 0);
        }
        // Costs are the real generated roster values, not HUD constants.
        let farm = menu.iter().find(|def| def.id == supply.id).unwrap();
        assert_eq!(farm.stats.cost, supply.stats.cost);
    }
    let human: Vec<_> = wc3_sim::data::build_menu(Race::Human)
        .iter()
        .map(|def| def.id)
        .collect();
    assert_eq!(human[0], "hhou");
    // Guard/Cannon towers are Scout Tower (`hwtw`) upgrades in the TFT tables
    // (`hgtw<-hwtw`, `hctw<-hwtw`), so the Human card carries the tower *base*
    // and the second slot is the cheapest producer (Arcane Sanctum).
    assert_eq!(human[1], "hars");
    assert!(
        human.contains(&"hwtw"),
        "the upgradeable Scout Tower base is on the card: {human:?}"
    );
    assert!(human[..4].contains(&"htow"), "hall in the first card row");
    assert!(
        human[..4].iter().any(|id| {
            wc3_sim::def_of(id)
                .map(|def| def.trains_units())
                .unwrap_or(false)
        }),
        "a producer in the first card row: {human:?}"
    );
}

#[test]
fn victory_and_defeat_follow_the_main_hall() {
    let mut sim = Sim::new(208, 32, 32);
    let _player_hall = hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let enemy_hall = hall(&mut sim, 1, Vec2::new(2600.0, 2600.0));
    sim.step(&[]);
    sim.kill_unit(enemy_hall);
    sim.step(&[]);
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::Victory(0)))
    );

    let mut sim = Sim::new(209, 32, 32);
    let player_hall = hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    hall(&mut sim, 1, Vec2::new(2600.0, 2600.0));
    sim.step(&[]);
    sim.kill_unit(player_hall);
    sim.step(&[]);
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::Victory(1)))
    );
}

#[test]
fn replaying_the_same_command_log_reproduces_the_state_hash() {
    fn run(seed: u64) -> u64 {
        let mut sim = Sim::new(seed, 32, 32);
        sim.passive_income = false;
        hall(&mut sim, 0, Vec2::new(400.0, 400.0));
        let worker = worker(&mut sim, 0, Vec2::new(600.0, 400.0));
        let foot = footman(&mut sim, 0, Vec2::new(700.0, 500.0));
        sim.add_gold_mine(Vec2::new(1000.0, 400.0), 5000);
        let script: Vec<(u32, Vec<wc3_sim::Command>)> = vec![
            (
                0,
                vec![wc3_sim::Command::Harvest {
                    units: vec![worker],
                    to: Vec2::new(1000.0, 400.0),
                }],
            ),
            (
                30,
                vec![wc3_sim::Command::Move {
                    units: vec![foot],
                    to: Vec2::new(1500.0, 900.0),
                }],
            ),
            (
                60,
                vec![wc3_sim::Command::MoveAttack {
                    units: vec![foot],
                    to: Vec2::new(400.0, 1200.0),
                }],
            ),
        ];
        for tick in 0..600 {
            let commands: Vec<_> = script
                .iter()
                .filter(|(at, _)| *at == tick)
                .flat_map(|(_, commands)| commands.clone())
                .collect();
            sim.step(&commands);
        }
        sim.state_hash()
    }
    assert_eq!(run(0x5eed), run(0x5eed));
}

#[test]
fn upgrades_cover_all_four_races_and_supported_effects() {
    for race in [Race::Human, Race::Orc, Race::Undead, Race::NightElf] {
        let upgrades: Vec<_> = wc3_sim::data::upgrades_for(race).collect();
        assert!(
            upgrades.len() >= 8,
            "{} upgrades: {}",
            race.name(),
            upgrades.len()
        );
        assert!(
            upgrades.iter().any(|upgrade| upgrade
                .effects
                .iter()
                .any(|effect| effect.kind == wc3_sim::data::UpgradeEffectKind::AttackDamage)),
            "{} has an attack upgrade",
            race.name()
        );
        assert!(
            upgrades.iter().any(|upgrade| upgrade
                .effects
                .iter()
                .any(|effect| effect.kind == wc3_sim::data::UpgradeEffectKind::Armor)),
            "{} has an armor upgrade",
            race.name()
        );
    }
}

#[test]
fn researching_an_upgrade_deducts_once_and_buffs_existing_and_new_units() {
    let mut sim = Sim::new(210, 32, 32);
    sim.passive_income = false;
    sim.resources[0] = 1000;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let blacksmith = sim.spawn_def(0, "hbla", Vec2::new(900.0, 400.0)).unwrap();
    let footman = sim.spawn_def(0, "hfoo", Vec2::new(700.0, 700.0)).unwrap();
    sim.step(&[]);
    let base_damage = sim.unit(footman).unwrap().damage;
    let upgrade = wc3_sim::data::upgrade_def("Rhme").expect("human melee attack");
    assert_eq!(upgrade.max_level, 3);
    sim.research_upgrade(blacksmith, "Rhme").expect("research");
    assert_eq!(sim.resources[0], 1000 - upgrade.gold_cost(1));
    assert_eq!(sim.unit(blacksmith).unwrap().research.len(), 1);
    let mut events = Vec::new();
    for _ in 0..upgrade.research_ticks(1) + 2 {
        sim.step(&[]);
        events.extend(sim.events.iter().cloned());
        if sim.upgrade_level(0, "Rhme") == 1 {
            break;
        }
    }
    assert_eq!(sim.upgrade_level(0, "Rhme"), 1);
    assert_eq!(sim.unit(footman).unwrap().damage, base_damage + 1);
    assert!(events.iter().any(|event| matches!(
        event,
        Event::UpgradeResearched {
            owner: 0,
            id: "Rhme"
        }
    )));

    // A unit trained after the upgrade inherits the bonus.
    let later = sim.spawn_def(0, "hfoo", Vec2::new(1100.0, 700.0)).unwrap();
    assert_eq!(sim.unit(later).unwrap().damage, base_damage + 1);

    // Second level costs the next tier and adds exactly one more damage.
    let cost = upgrade.gold_cost(2);
    assert!(sim.resources[0] >= cost);
    sim.research_upgrade(blacksmith, "Rhme").expect("level 2");
    for _ in 0..upgrade.research_ticks(2) + 2 {
        sim.step(&[]);
    }
    assert_eq!(sim.upgrade_level(0, "Rhme"), 2);
    assert_eq!(sim.unit(footman).unwrap().damage, base_damage + 2);

    // Insufficient gold rejects without changing the level.
    sim.resources[0] = 0;
    assert_eq!(
        sim.research_upgrade(blacksmith, "Rhme"),
        Err("not enough gold")
    );
    assert_eq!(sim.upgrade_level(0, "Rhme"), 2);
}

#[test]
fn item_table_is_complete_and_items_apply_real_bonuses() {
    assert_eq!(wc3_sim::data::ITEMS.len(), 273);
    assert!(
        wc3_sim::data::ITEMS
            .iter()
            .filter(|item| item.has_effect())
            .count()
            >= 20
    );

    let mut sim = Sim::new(211, 32, 32);
    sim.passive_income = false;
    let unit = footman(&mut sim, 0, Vec2::new(500.0, 500.0));
    sim.step(&[]);
    let base_armor = sim.unit(unit).unwrap().armor;
    let ring = sim
        .spawn_item("rde4", Vec2::new(520.0, 500.0), 0)
        .expect("ring of protection");
    sim.pickup_item(unit, ring).expect("pickup");
    assert_eq!(sim.unit(unit).unwrap().armor, base_armor + 5);
    assert!(sim.unit(unit).unwrap().items.contains(&ring));
    sim.drop_item(unit, ring).expect("drop");
    assert_eq!(sim.unit(unit).unwrap().armor, base_armor);

    // Consumable: heal, decrement charge, disappear at zero.
    sim.set_unit_hp(unit, 100);
    let potion = sim
        .spawn_item("rej1", Vec2::new(520.0, 500.0), 1)
        .expect("potion");
    sim.pickup_item(unit, potion).expect("pickup potion");
    sim.use_item(unit, potion).expect("drink");
    assert!(sim.unit(unit).unwrap().hp > 100);
    assert!(sim.item(potion).is_none(), "empty potion removed");

    // Passive-only items cannot be "used".
    let ring = sim
        .spawn_item("rde4", Vec2::new(520.0, 500.0), 0)
        .expect("ring");
    sim.pickup_item(unit, ring).expect("pickup ring");
    assert_eq!(
        sim.use_item(unit, ring),
        Err("item has no implemented effect")
    );
}

#[test]
fn every_roster_caster_can_cast_its_listed_spells() {
    for def_id in [
        "Hpal", "Hamg", "Hmkg", "Ofar", "Udea", "Ulic", "Udre", "Ekee",
    ] {
        let def = wc3_sim::def_of(def_id).expect("caster");
        assert!(!def.spells.is_empty(), "{def_id} has spells");
        for spell in def.spells {
            assert_eq!(
                wc3_sim::Spell::from_id(spell.def().id),
                Some(*spell),
                "{def_id} {} is registered",
                spell.name()
            );
        }
    }
}

#[test]
fn new_hero_spells_have_real_effects() {
    // Mountain King: Storm Bolt damages and stuns.
    let mut sim = Sim::new(212, 32, 32);
    let king = sim.spawn_def(0, "Hmkg", Vec2::new(500.0, 500.0)).unwrap();
    let victim = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    let hp = sim.unit(victim).unwrap().hp;
    sim.cast(
        king,
        wc3_sim::Spell::StormBolt,
        wc3_sim::SpellTarget::Unit(victim),
    )
    .expect("storm bolt");
    assert!(sim.unit(victim).unwrap().hp < hp);
    assert!(sim.unit(victim).unwrap().stunned_until > sim.tick);
    let pos = sim.unit(victim).unwrap().pos;
    sim.step(&[]);
    assert_eq!(
        sim.unit(victim).unwrap().pos,
        pos,
        "stunned unit cannot move"
    );

    // Mountain King: Thunder Clap damages enemies around the caster.
    let mut sim = Sim::new(213, 32, 32);
    let king = sim.spawn_def(0, "Hmkg", Vec2::new(500.0, 500.0)).unwrap();
    let a = footman(&mut sim, 1, Vec2::new(640.0, 500.0));
    let b = footman(&mut sim, 1, Vec2::new(500.0, 640.0));
    sim.cast(
        king,
        wc3_sim::Spell::ThunderClap,
        wc3_sim::SpellTarget::None,
    )
    .expect("thunder clap");
    assert!(sim.unit(a).unwrap().hp < sim.unit(a).unwrap().max_hp);
    assert!(sim.unit(b).unwrap().hp < sim.unit(b).unwrap().max_hp);

    // Death Knight: Death Coil damages an enemy and heals an ally.
    let mut sim = Sim::new(214, 32, 32);
    let knight = sim.spawn_def(0, "Udea", Vec2::new(500.0, 500.0)).unwrap();
    let ally = sim.spawn_def(0, "ugho", Vec2::new(700.0, 500.0)).unwrap();
    let enemy = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.set_unit_hp(ally, 100);
    sim.cast(
        knight,
        wc3_sim::Spell::DeathCoil,
        wc3_sim::SpellTarget::Unit(ally),
    )
    .expect("death coil heal");
    assert!(sim.unit(ally).unwrap().hp > 100);
    step_n(&mut sim, 160); // Death Coil cooldown
    let hp = sim.unit(enemy).unwrap().hp;
    sim.cast(
        knight,
        wc3_sim::Spell::DeathCoil,
        wc3_sim::SpellTarget::Unit(enemy),
    )
    .expect("death coil damage");
    assert!(sim.unit(enemy).unwrap().hp < hp);

    // Lich: Frost Nova hits the target and nearby enemies.
    let mut sim = Sim::new(215, 32, 32);
    let lich = sim.spawn_def(0, "Ulic", Vec2::new(500.0, 500.0)).unwrap();
    let a = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    let b = footman(&mut sim, 1, Vec2::new(1000.0, 500.0));
    sim.cast(
        lich,
        wc3_sim::Spell::FrostNova,
        wc3_sim::SpellTarget::Unit(a),
    )
    .expect("frost nova");
    assert!(sim.unit(a).unwrap().hp < sim.unit(a).unwrap().max_hp);
    assert!(sim.unit(b).unwrap().hp < sim.unit(b).unwrap().max_hp);

    // Dread Lord: Carrion Swarm damages a point area.
    let mut sim = Sim::new(216, 32, 32);
    let lord = sim.spawn_def(0, "Udre", Vec2::new(500.0, 500.0)).unwrap();
    let victim = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.cast(
        lord,
        wc3_sim::Spell::CarrionSwarm,
        wc3_sim::SpellTarget::Point(Vec2::new(900.0, 500.0)),
    )
    .expect("carrion swarm");
    assert!(sim.unit(victim).unwrap().hp < sim.unit(victim).unwrap().max_hp);
}

#[test]
fn unit_identity_is_the_def_id_not_the_four_kind_bucket() {
    let mut sim = Sim::new(217, 32, 32);
    let footman = sim.spawn_def(0, "hfoo", Vec2::new(400.0, 400.0)).unwrap();
    let knight = sim.spawn_def(0, "hkni", Vec2::new(600.0, 400.0)).unwrap();
    let rifleman = sim.spawn_def(0, "hrif", Vec2::new(800.0, 400.0)).unwrap();
    assert_eq!(sim.unit(footman).unwrap().kind, UnitKind::Footman);
    assert_eq!(sim.unit(knight).unwrap().kind, UnitKind::Footman);
    assert_ne!(
        sim.unit(footman).unwrap().def_id(),
        sim.unit(knight).unwrap().def_id()
    );
    assert_eq!(
        sim.unit(footman).unwrap().def_id(),
        UnitDefId::from_id("hfoo").unwrap()
    );
    assert_eq!(sim.unit(rifleman).unwrap().kind, UnitKind::Archer);
    assert_ne!(
        sim.unit(footman).unwrap().max_hp,
        sim.unit(knight).unwrap().max_hp
    );
}

#[test]
fn heroes_across_all_four_races_are_altar_trainable() {
    // Re-derived from the data instead of pinned to a count: a hero def names
    // its producer in `requires`, the same producer must appear in
    // `trained_at`, and the altar/tavern card is exactly the roster row's
    // `heroes_of_building` list. The total is whatever the roster supplies
    // today (16 melee + 5 tavern), so adding a hero moves the number instead
    // of failing the test — while a hero without a producer still does.
    let heroes: Vec<_> = wc3_sim::ROSTER.iter().filter(|def| def.hero).collect();
    assert!(!heroes.is_empty(), "the roster has heroes");
    for hero in &heroes {
        assert!(
            !hero.requires.is_empty(),
            "{} has no producer in `requires`",
            hero.id
        );
        assert!(
            hero.requires.iter().all(|producer| hero
                .trained_at
                .contains(producer)),
            "{} requires a producer it is not trained at: requires {:?}, trained_at {:?}",
            hero.id,
            hero.requires,
            hero.trained_at
        );
        assert!(
            hero.trained_at.iter().any(|producer| {
                wc3_sim::def_of(producer)
                    .map(|def| def.is_building())
                    .unwrap_or(false)
            }),
            "{} is not trained by a building",
            hero.id
        );
    }

    let mut sim = Sim::new(219, 64, 64);
    let mut total = 0usize;
    for (slot, race) in [
        Race::Human,
        Race::Orc,
        Race::Undead,
        Race::NightElf,
        Race::Neutral,
    ]
    .into_iter()
    .enumerate()
    {
        let producer = wc3_sim::data::altar_for(race)
            .unwrap_or_else(|| panic!("{} has no altar/tavern", race.name()));
        let pos = Vec2::new(500.0 + slot as f32 * 900.0, 500.0);
        let altar = sim
            .spawn_def_exact(0, producer.id, pos)
            .unwrap_or_else(|| panic!("spawn {}", producer.id));
        let menu = sim.altar_menu(altar);
        let expected: Vec<&str> = heroes
            .iter()
            .filter(|def| def.trained_at.contains(&producer.id))
            .map(|def| def.id)
            .collect();
        assert_eq!(
            menu.len(),
            expected.len(),
            "{} trains {} heroes, card has {}",
            producer.id,
            expected.len(),
            menu.len()
        );
        for id in &expected {
            assert!(
                menu.iter().any(|def| def.id == *id),
                "{id} is trainable at {} but missing from its card",
                producer.id
            );
        }
        for def in &menu {
            assert!(
                def.requires.contains(&producer.id),
                "{} is on the {} card but does not require it",
                def.id,
                producer.id
            );
        }
        total += menu.len();
    }
    assert_eq!(
        total,
        heroes.len(),
        "every hero is on exactly one altar/tavern card"
    );

    assert!(wc3_sim::def_of("Hmkg").unwrap().hero);
    assert!(!wc3_sim::def_of("hfoo").unwrap().hero);
}

#[test]
fn lumber_cycle_chops_a_tree_and_banks_ten_per_trip() {
    let mut sim = Sim::new(218, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let peasant = worker(&mut sim, 0, Vec2::new(600.0, 400.0));
    let _tree = sim
        .spawn_destructable("LTlt", Vec2::new(1100.0, 400.0))
        .expect("tree");
    assert_eq!(sim.destructables[0].hp, 50);
    let before = sim.lumber[0];
    sim.harvest_lumber(&[peasant], Vec2::new(1100.0, 400.0));
    let mut events = Vec::new();
    for _ in 0..1800 {
        sim.step(&[]);
        events.extend(sim.events.iter().cloned());
        if sim.lumber[0] >= before + 20 {
            break;
        }
    }
    assert!(sim.lumber[0] >= before + 20, "lumber {}", sim.lumber[0]);
    assert_eq!(sim.lumber[0] % 10, 0);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::LumberDeposited { owner: 0, .. }))
    );
    assert!(sim.destructables[0].hp < 50, "tree lost HP");
}

#[test]
fn destructable_hp_and_cell_release() {
    let mut sim = Sim::new(219, 32, 32);
    let pos = Vec2::new(900.0, 900.0);
    let tree = sim.spawn_destructable("LTlt", pos).expect("tree");
    let cell = sim.grid.world_to_cell(pos);
    assert!(!sim.grid.walkable(cell.0, cell.1));
    assert_eq!(sim.damage_destructable(tree, 20), 20);
    assert_eq!(sim.destructables[0].hp, 30);
    assert!(sim.destructables[0].alive);
    assert_eq!(sim.damage_destructable(tree, 40), 30);
    assert!(!sim.destructables[0].alive);
    assert!(sim.grid.walkable(cell.0, cell.1));
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::DestructableDestroyed { id } if *id == tree))
    );
}

#[test]
fn lumber_costs_are_enforced_for_training_and_building() {
    let mut sim = Sim::new(220, 32, 32);
    sim.passive_income = false;
    sim.resources[0] = 10_000;
    sim.lumber[0] = 0;
    let hall_id = hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let builder = worker(&mut sim, 0, Vec2::new(700.0, 400.0));
    // Human barracks costs 80 lumber; with zero lumber it must be rejected.
    assert_eq!(
        sim.place_building(builder, "hbar", Vec2::new(1000.0, 400.0)),
        Err("not enough lumber")
    );
    // Footman training is lumber-free; the hall accepts it.
    sim.train_def(hall_id, "hpea")
        .expect("peasant has no lumber cost");
    sim.lumber[0] = 1000;
    sim.place_building(builder, "hbar", Vec2::new(1000.0, 400.0))
        .expect("barracks with lumber");
    assert_eq!(sim.lumber[0], 1000 - 80);
}

#[test]
fn creep_camp_aggros_within_500_and_leashes_home() {
    let mut sim = Sim::new(221, 64, 64);
    let player = footman(&mut sim, 0, Vec2::new(2400.0, 2000.0));
    let camp = sim
        .spawn_camp(&["nftr", "nban"], Vec2::new(2000.0, 2000.0), 500.0, 900.0)
        .expect("camp");
    assert_eq!(sim.camps.len(), 1);
    assert_eq!(sim.camps[0].units.len(), 2);
    assert!(
        sim.camps[0]
            .units
            .iter()
            .all(|unit| { sim.unit(*unit).map(|unit| unit.owner == 2).unwrap_or(false) })
    );
    let mut aggro = false;
    for _ in 0..60 {
        sim.step(&[]);
        if sim.camps[0]
            .units
            .iter()
            .filter_map(|unit| sim.unit(*unit))
            .any(|unit| matches!(unit.order, Order::Attack(_)))
        {
            aggro = true;
            break;
        }
    }
    assert!(aggro, "camp aggro at <=500");
    // Teleport the player far outside the leash; the camp returns home.
    sim.teleport_unit(player, Vec2::new(5200.0, 2000.0));
    for _ in 0..900 {
        sim.step(&[]);
    }
    let home = sim.camps[0]
        .units
        .iter()
        .filter_map(|unit| sim.unit(*unit))
        .filter(|unit| unit.alive)
        .all(|unit| unit.pos.distance(Vec2::new(2000.0, 2000.0)) <= 950.0);
    assert!(home, "camp leashed back to the anchor");
    assert_eq!(camp, 1);
}

#[test]
fn map_placed_units_spawn_with_owner_and_position() {
    let mut sim = Sim::new(222, 64, 64);
    let ids = sim.spawn_map_units(&[
        (0, "hfoo", Vec2::new(600.0, 600.0)),
        (1, "ugho", Vec2::new(3000.0, 3000.0)),
        (2, "nftr", Vec2::new(2000.0, 2000.0)),
    ]);
    assert_eq!(ids.len(), 3);
    assert_eq!(sim.unit(ids[0]).unwrap().owner, 0);
    assert_eq!(sim.unit(ids[1]).unwrap().owner, 1);
    assert_eq!(sim.unit(ids[2]).unwrap().owner, 2);
    assert!((sim.unit(ids[2]).unwrap().pos - Vec2::new(2000.0, 2000.0)).length() < 2.0);
}

#[test]
fn building_placement_enforces_the_real_tier_requirements() {
    let mut sim = Sim::new(223, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 50_000;
    sim.lumber[0] = 50_000;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let builder = worker(&mut sim, 0, Vec2::new(900.0, 400.0));
    sim.step(&[]);
    assert_eq!(
        sim.place_building(builder, "hcas", Vec2::new(1200.0, 400.0)),
        Err("missing requirement"),
        "castle without keep is denied"
    );
    // Keep (requires town hall) unlocks the castle.
    sim.spawn_def(0, "hkee", Vec2::new(3000.0, 900.0));
    sim.step(&[]);
    sim.place_building(builder, "hcas", Vec2::new(1200.0, 400.0))
        .expect("castle with keep");
}

#[test]
fn roc_and_tft_rulesets_switch_caps_and_upkeep() {
    let mut sim = Sim::new(224, 64, 64);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    // 21 footmen = 42 supply.
    for i in 0..21 {
        footman(&mut sim, 0, Vec2::new(600.0 + (i % 5) as f32 * 40.0, 900.0));
    }
    sim.step(&[]);
    assert_eq!(sim.supply_used(0), 42);
    sim.set_ruleset(wc3_sim::Ruleset::Roc);
    assert_eq!(sim.upkeep_permille(0), 700, "RoC low upkeep starts at 41");
    assert_eq!(sim.supply_cap(0), 12, "cap from hall still applies");
    sim.set_ruleset(wc3_sim::Ruleset::Tft);
    assert_eq!(sim.upkeep_permille(0), 1000, "TFT no upkeep at 42");

    // RoC hard cap is 90, TFT 100: prove with enough halls.
    for i in 0..9 {
        sim.spawn_def(0, "htow", Vec2::new(1500.0 + i as f32 * 300.0, 1500.0));
    }
    sim.set_ruleset(wc3_sim::Ruleset::Roc);
    assert!(sim.supply_cap(0) <= 90);
    sim.set_ruleset(wc3_sim::Ruleset::Tft);
    assert!(sim.supply_cap(0) <= 100);
}

#[test]
fn qa_creeps_camps() {
    let mut sim = Sim::new(221, 64, 64);
    let player = footman(&mut sim, 0, Vec2::new(2400.0, 2000.0));
    let _camp = sim
        .spawn_camp(&["nftr", "nban"], Vec2::new(2000.0, 2000.0), 500.0, 900.0)
        .expect("camp");
    assert_eq!(sim.camps.len(), 1);
    assert_eq!(sim.camps[0].units.len(), 2);
    let mut aggro = false;
    for _ in 0..60 {
        sim.step(&[]);
        if sim.camps[0]
            .units
            .iter()
            .filter_map(|unit| sim.unit(*unit))
            .any(|unit| matches!(unit.order, Order::Attack(_)))
        {
            aggro = true;
            break;
        }
    }
    assert!(aggro, "camp aggro at <=500");
    sim.teleport_unit(player, Vec2::new(5200.0, 2000.0));
    for _ in 0..900 {
        sim.step(&[]);
    }
    for unit in sim.camps[0].units.clone() {
        let unit = sim.unit(unit).expect("creep");
        assert!(
            unit.pos.distance(Vec2::new(2000.0, 2000.0)) <= 1100.0,
            "leash home: dist {} order {:?}",
            unit.pos.distance(Vec2::new(2000.0, 2000.0)),
            unit.order
        );
    }
}

#[test]
fn build_requires() {
    // hcas requires hkee (Castle requires Keep).
    let mut sim = Sim::new(223, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 50_000;
    sim.lumber[0] = 50_000;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let builder = worker(&mut sim, 0, Vec2::new(900.0, 400.0));
    sim.step(&[]);
    assert_eq!(
        sim.place_building(builder, "hcas", Vec2::new(1200.0, 400.0)),
        Err("missing requirement"),
        "castle without keep is denied"
    );
    // Keep unlocks the castle.
    sim.spawn_def(0, "hkee", Vec2::new(3000.0, 900.0));
    sim.step(&[]);
    assert!(
        sim.place_building(builder, "hcas", Vec2::new(1200.0, 400.0))
            .is_ok(),
        "castle with keep"
    );
}

#[test]
fn ruleset_roc() {
    let mut sim = Sim::new(225, 64, 64);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.set_ruleset(wc3_sim::Ruleset::Roc);
    // RoC upkeep tiers: 0-40 no upkeep, 41-70 low, 71-90 high.
    assert_eq!(sim.upkeep_permille(0), 1000);
    assert_eq!(sim.supply_cap(0), 12);
    for i in 0..21 {
        footman(&mut sim, 0, Vec2::new(600.0 + (i % 5) as f32 * 40.0, 900.0));
    }
    sim.step(&[]);
    assert_eq!(sim.supply_used(0), 42);
    assert_eq!(sim.upkeep_permille(0), 700, "RoC low upkeep 41-70");
    // RoC hard cap 90.
    for i in 0..9 {
        sim.spawn_def(0, "htow", Vec2::new(1500.0 + i as f32 * 300.0, 1500.0));
    }
    sim.set_ruleset(wc3_sim::Ruleset::Roc);
    assert!(sim.supply_cap(0) <= 90);
}

#[test]
fn ruleset_tft() {
    let mut sim = Sim::new(226, 64, 64);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    sim.set_ruleset(wc3_sim::Ruleset::Tft);
    assert_eq!(sim.upkeep_permille(0), 1000);
    assert_eq!(sim.supply_cap(0), 12);
    for i in 0..26 {
        footman(&mut sim, 0, Vec2::new(600.0 + (i % 5) as f32 * 40.0, 900.0));
    }
    sim.step(&[]);
    assert_eq!(sim.supply_used(0), 52);
    assert_eq!(sim.upkeep_permille(0), 700, "TFT low upkeep 51-80");
    // TFT hard cap 100.
    for i in 0..10 {
        sim.spawn_def(0, "htow", Vec2::new(1500.0 + i as f32 * 300.0, 1500.0));
    }
    sim.set_ruleset(wc3_sim::Ruleset::Tft);
    assert!(sim.supply_cap(0) <= 100);
}

#[test]
fn neutral_map_units_cluster_into_creep_camps() {
    let mut sim = Sim::new(227, 64, 64);
    // Two proximity camps of two creeps plus one lone neutral unit.
    sim.spawn_def(2, "nftr", Vec2::new(600.0, 600.0));
    sim.spawn_def(2, "nban", Vec2::new(650.0, 620.0));
    sim.spawn_def(2, "nftr", Vec2::new(3000.0, 600.0));
    sim.spawn_def(2, "nban", Vec2::new(3040.0, 640.0));
    sim.spawn_def(2, "nftr", Vec2::new(5200.0, 5200.0));
    let created = sim.cluster_neutral_camps();
    assert_eq!(created, 3, "three proximity groups");
    assert_eq!(sim.camps.len(), 3);
    assert_eq!(sim.cluster_neutral_camps(), 0, "second pass is idempotent");
    // A player unit inside 500 of the first camp wakes it; the others stay idle.
    footman(&mut sim, 0, Vec2::new(1000.0, 600.0));
    let mut aggro = false;
    for _ in 0..120 {
        sim.step(&[]);
        if sim.camps[0]
            .units
            .iter()
            .filter_map(|unit| sim.unit(*unit))
            .any(|unit| matches!(unit.order, Order::Attack(_)))
        {
            aggro = true;
            break;
        }
    }
    assert!(aggro, "clustered camp aggro at <=500");
    assert!(
        sim.camps[1]
            .units
            .iter()
            .filter_map(|unit| sim.unit(*unit))
            .all(|unit| !matches!(unit.order, Order::Attack(_))),
        "distant camp stays idle"
    );
}
