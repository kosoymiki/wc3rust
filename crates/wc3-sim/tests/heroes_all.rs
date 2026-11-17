//! Hero roster, altar/tavern menus, ability mapping and progression.
//!
//! One test per mechanic id:
//! * `hero.roster.all-heroes`
//! * `hero.altar.menu-per-race`
//! * `hero.ability.mapping-coverage`
//! * `hero.learn.spends-point`
//! * `hero.revive.altar`
//! * `hero.xp.kill-level`

use std::collections::BTreeSet;

use wc3_sim::abilities::{self, Spell};
use wc3_sim::data::{self, ROSTER};
use wc3_sim::heroes;
use wc3_sim::{AttackType, Command, Event, HeroAttribute, Race, Sim, Vec2};

/// WC3 TFT melee hero line: four per race.
const MELEE_HEROES: usize = 16;
/// Tavern roster present in the local TFT extract: Nbst, Nbrn, Nngs, Npbm,
/// Nplh (`Sellunits=` in `NeutralUnitFunc.txt`).  The 1.0x archive predates
/// Firelord/Tinker/Alchemist rows, so those three cannot be generated from the
/// extract without inventing data.
const TAVERN_HEROES: usize = 5;

fn hero_lines() -> Vec<(&'static str, Race)> {
    vec![
        ("halt", Race::Human),
        ("oalt", Race::Orc),
        ("uaod", Race::Undead),
        ("eate", Race::NightElf),
    ]
}

/// `hero.roster.all-heroes`: every hero definition is a real playable hero —
/// race, altar/tavern linkage, primary attribute, growth, icon, model and the
/// full ability list from `UnitAbilities.slk`.
#[test]
fn hero_roster_all_heroes() {
    let heroes: Vec<_> = heroes::all_heroes().collect();
    println!(
        "heroes: {} (melee {} + tavern {}), roster defs {}",
        heroes.len(),
        MELEE_HEROES,
        TAVERN_HEROES,
        ROSTER.len()
    );
    assert_eq!(heroes.len(), MELEE_HEROES + TAVERN_HEROES);

    for (altar_id, race) in hero_lines() {
        let line: Vec<&'static str> = heroes::race_heroes(race).iter().map(|def| def.id).collect();
        println!("{race:?} altar {altar_id}: {line:?}");
        assert_eq!(line.len(), 4, "{race:?} altar line");
    }
    let tavern: Vec<&'static str> = heroes::tavern_heroes().iter().map(|def| def.id).collect();
    println!("tavern ntav: {tavern:?}");
    assert_eq!(tavern.len(), TAVERN_HEROES);

    for def in heroes {
        assert_ne!(
            def.stats.hero_primary,
            HeroAttribute::None,
            "{}: no primary attribute",
            def.id
        );
        assert!(
            def.stats.hero_strength > 0
                && def.stats.hero_agility > 0
                && def.stats.hero_intelligence > 0,
            "{}: base attributes",
            def.id
        );
        assert!(
            def.stats.hero_strength_gain > 0.0
                && def.stats.hero_agility_gain > 0.0
                && def.stats.hero_intelligence_gain > 0.0,
            "{}: per-level growth",
            def.id
        );
        let altar_id = if def.race == Race::Neutral {
            "ntav"
        } else {
            data::altar_for(def.race).expect("race altar").id
        };
        assert_eq!(def.requires, &[altar_id][..], "{}: requires", def.id);
        assert_eq!(def.trained_at, &[altar_id][..], "{}: trained_at", def.id);
        let altar = data::def_of(altar_id).expect("altar def");
        assert!(
            altar.trains.contains(&def.id),
            "{}: altar {} trains list",
            def.id,
            altar_id
        );
        assert!(
            def.ability_ids.len() >= 4,
            "{}: only {} ability ids",
            def.id,
            def.ability_ids.len()
        );
        assert!(!def.icon.is_empty(), "{}: icon", def.id);
        assert!(def.model_present, "{}: model not in the archive", def.id);
    }
}

/// `hero.altar.menu-per-race`: each race altar lists exactly its four heroes,
/// the tavern lists its own neutral roster, and `Sim::trainable_heroes(owner)`
/// expands the owned buildings into the card list.
#[test]
fn hero_altar_menu_per_race() {
    let mut sim = Sim::new(4461, 96, 96);
    sim.passive_income = false;
    let mut y = 800.0;
    let mut human_altar = None;
    for (altar_id, race) in hero_lines() {
        let altar = sim
            .spawn_def(0, altar_id, Vec2::new(800.0, y))
            .unwrap_or_else(|| panic!("spawn {altar_id}"));
        if race == Race::Human {
            human_altar = Some(altar);
        }
        let menu: Vec<&'static str> = sim.altar_menu(altar).iter().map(|def| def.id).collect();
        println!("{race:?} altar {altar_id} menu: {menu:?}");
        assert_eq!(menu.len(), 4, "{altar_id} menu");
        for def in sim.altar_menu(altar) {
            assert_eq!(def.race, race, "{} on the {altar_id} card", def.id);
        }
        assert_eq!(heroes::heroes_of_building(altar_id).len(), 4);
        y += 700.0;
    }
    let trainable: Vec<&'static str> = sim
        .trainable_heroes(0)
        .iter()
        .map(|def| def.id)
        .collect();
    assert_eq!(
        trainable.len(),
        MELEE_HEROES,
        "owned altars must expose all melee heroes: {trainable:?}"
    );

    let tavern = sim
        .spawn_def(0, "ntav", Vec2::new(3000.0, 800.0))
        .expect("tavern");
    let tavern_menu: Vec<&'static str> = sim.altar_menu(tavern).iter().map(|def| def.id).collect();
    println!("tavern menu: {tavern_menu:?}");
    assert_eq!(tavern_menu.len(), TAVERN_HEROES);
    assert!(sim
        .altar_menu(tavern)
        .iter()
        .all(|def| def.race == Race::Neutral));
    assert_eq!(sim.trainable_heroes(0).len(), MELEE_HEROES + TAVERN_HEROES);
    // A hero the owner does not own is not on the card.
    assert!(sim.trainable_heroes(1).is_empty());

    // The altar really trains the hero it lists (`Trains=` -> train_def).
    let altar = human_altar.expect("human altar");
    sim.resources[0] = 5_000;
    sim.lumber[0] = 1_000;
    sim.train_def(altar, "Hpal")
        .expect("altar queues the Paladin");
    assert!(
        !sim.unit(altar).unwrap().production.is_empty(),
        "altar production queue"
    );
}

/// `hero.ability.mapping-coverage`: every hero ability id in
/// `UnitAbilities.slk` resolves either to a typed `Spell` or to an explicit
/// data-backed / `DataOnly` tag; nothing is silently unknown.
#[test]
fn hero_ability_mapping_coverage() {
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    let (mut typed, mut effect, mut data_only, mut unknown) = (0usize, 0usize, 0usize, 0usize);
    let mut unknown_ids: Vec<&'static str> = Vec::new();
    let mut categories = std::collections::BTreeMap::new();
    for def in heroes::all_heroes() {
        for id in def.ability_ids {
            if !seen.insert(id) {
                continue;
            }
            match abilities::spell_for_ability(id) {
                Some(Spell::DataOnly { .. }) => {
                    data_only += 1;
                    *categories.entry("data-only").or_insert(0) += 1;
                }
                Some(spell) if spell.is_typed() => {
                    typed += 1;
                    *categories.entry("typed").or_insert(0) += 1;
                }
                Some(spell) => {
                    effect += 1;
                    let tag = match spell {
                        Spell::DataDamage { .. } => "damage",
                        Spell::DataHeal { .. } => "heal",
                        Spell::DataBuff { .. } => "buff",
                        Spell::DataSummon { .. } => "summon",
                        Spell::DataAura { .. } => "aura",
                        Spell::DataAuraPassive { .. } => "aura-passive",
                        Spell::DataTransformation { .. } => "transformation",
                        _ => "other",
                    };
                    *categories.entry(tag).or_insert(0) += 1;
                }
                None => {
                    unknown += 1;
                    unknown_ids.push(id);
                }
            }
        }
    }
    println!(
        "hero ability ids: {} typed={} data-effect={} data-only={} unknown={}",
        seen.len(),
        typed,
        effect,
        data_only,
        unknown
    );
    println!("categories: {categories:?}");
    if !unknown_ids.is_empty() {
        println!("UNKNOWN: {unknown_ids:?}");
    }
    assert_eq!(unknown, 0, "unknown hero abilities: {unknown_ids:?}");
    assert!(
        typed >= 10,
        "the hand-written hero spells must stay mapped: {typed}"
    );
    assert!(effect > 0, "data rows with a real handler must map to a Spell");
    let resolved = typed + effect + data_only;
    assert_eq!(resolved, seen.len());
    assert!(
        resolved * 100 >= seen.len() * 90,
        "coverage {resolved}/{}",
        seen.len()
    );
    // The known hand-written ids keep their typed variants; `Ahea` is the
    // Priest unit spell, not on a hero list, so typed hero ids are 10 of 11.
    assert_eq!(abilities::spell_for_ability("AHhb"), Some(Spell::HolyLight));
    assert_eq!(abilities::spell_for_ability("Ahea"), Some(Spell::Heal));
    assert_eq!(abilities::spell_for_ability("AEer"), Some(Spell::EntanglingRoots));
    // An id that exists in the sheet but has no handler is an explicit tag.
    assert!(matches!(
        abilities::spell_for_ability("AEev"),
        Some(Spell::DataOnly { .. })
    ));
    assert!(abilities::spell_for_ability("not_an_id").is_none());

    // Casting a DataOnly ability is refused and logged, never a silent no-op.
    let mut sim = Sim::new(4465, 64, 64);
    sim.passive_income = false;
    let mage = sim
        .spawn_def(0, "Hamg", Vec2::new(500.0, 500.0))
        .expect("archmage");
    let mana = sim.unit(mage).unwrap().mana;
    assert_eq!(
        sim.cast(
            mage,
            Spell::DataOnly { id: "AOwk" },
            wc3_sim::SpellTarget::Point(Vec2::new(560.0, 500.0))
        ),
        Err("ABILITY_DATA_ONLY")
    );
    assert_eq!(sim.unit(mage).unwrap().mana, mana, "no mana for a no-op");

    // A data-mapped damage ability casts end-to-end through the same Spell
    // value the card would show: Shock Wave (Chieftain AOsh) hits enemies in
    // its 150 radius at the target point.
    let mut sim = Sim::new(4466, 64, 64);
    sim.passive_income = false;
    let chief = sim
        .spawn_def(0, "Otch", Vec2::new(500.0, 500.0))
        .expect("tauren chieftain");
    let victim = sim
        .spawn_def(1, "hfoo", Vec2::new(620.0, 500.0))
        .expect("footman");
    sim.learn_ability(chief, "AOsh").expect("shock wave rank 1");
    let hp = sim.unit(victim).unwrap().hp;
    let spell = abilities::spell_for_ability("AOsh").expect("AOsh maps");
    assert!(matches!(spell, Spell::DataDamage { .. }), "{spell:?}");
    sim.cast(
        chief,
        spell,
        wc3_sim::SpellTarget::Point(Vec2::new(620.0, 500.0)),
    )
    .expect("data damage cast");
    assert!(
        sim.unit(victim).unwrap().hp < hp,
        "Shock Wave must damage: {hp} -> {}",
        sim.unit(victim).unwrap().hp
    );
}

/// `hero.learn.spends-point`: `Command::LearnAbility` buys a rank, decrements
/// the point, respects `reqLevel`, and the LEARN submenu exposes the rows.
#[test]
fn hero_learn_spends_point() {
    let mut sim = Sim::new(4462, 64, 64);
    sim.passive_income = false;
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(420.0, 500.0))
        .expect("paladin");
    let menu = sim.hero_ability_menu(hero);
    println!(
        "LEARN rows: {:?}",
        menu.iter()
            .map(|row| (row.id, row.req_level, row.learned))
            .collect::<Vec<_>>()
    );
    assert!(menu.iter().any(|row| row.id == "AHhb" && row.req_level == 1));
    assert!(menu.iter().any(|row| row.id == "AHre" && row.req_level == 6));
    assert_eq!(sim.unit(hero).unwrap().skill_points, 1);

    sim.step(&[Command::LearnAbility {
        unit: hero,
        ability: "AHhb",
    }]);
    assert!(sim.unit(hero).unwrap().learned.contains(&"AHhb"));
    assert_eq!(sim.unit(hero).unwrap().skill_points, 0);
    assert_eq!(
        sim.learn_ability(hero, "AHhb"),
        Err("no skill points")
    );

    // Ultimate gated at hero level 6; level-up adds one point per level.
    sim.set_hero_level(hero, 6).expect("level 6");
    assert_eq!(sim.unit(hero).unwrap().skill_points, 5);
    sim.learn_ability(hero, "AHre").expect("ultimate at 6");
    sim.learn_ability(hero, "AHds").expect("second basic rank");
    assert_eq!(sim.unit(hero).unwrap().skill_points, 3);

    let fresh = sim
        .spawn_def(0, "Hpal", Vec2::new(520.0, 500.0))
        .expect("second paladin");
    assert_eq!(
        sim.learn_ability(fresh, "AHre"),
        Err("hero level too low")
    );
}

/// `hero.revive.altar`: the altar revive command charges 700 gold, waits the
/// 150 s timer and brings the hero back at the altar; the tavern revives its
/// own neutral heroes the same way.
#[test]
fn hero_revive_altar() {
    let mut sim = Sim::new(4463, 64, 64);
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
    assert_eq!(sim.revive_candidates(altar), vec![hero]);

    sim.step(&[Command::ReviveHero { hero, altar }]);
    assert_eq!(sim.resources[0], 5_000 - 700, "HeroMaxReviveCostGold");
    assert!(sim.unit(hero).unwrap().revive_at > sim.tick);
    let mut revived = false;
    for _ in 0..(150 * 30 + 30) {
        sim.step(&[]);
        if sim.events.iter().any(
            |event| matches!(event, Event::HeroRevived { hero: id, .. } if *id == hero),
        ) {
            revived = true;
            break;
        }
    }
    assert!(revived, "HeroMaxReviveTime = 150 s");
    let unit = sim.unit(hero).expect("hero");
    assert!(unit.alive);
    assert_eq!(unit.hp, unit.max_hp);

    // Tavern heroes revive at the tavern (`altar_for(Neutral) == ntav`).
    let tavern = sim
        .spawn_def(0, "ntav", Vec2::new(1200.0, 1200.0))
        .expect("tavern");
    let brewmaster = sim
        .spawn_def(0, "Npbm", Vec2::new(1300.0, 1200.0))
        .expect("brewmaster");
    sim.apply_damage(brewmaster, brewmaster, 100_000, AttackType::Magic);
    assert_eq!(sim.revive_candidates(tavern), vec![brewmaster]);
    sim.revive_hero(brewmaster, tavern).expect("tavern revive");
}

/// `hero.xp.kill-level`: a kill grants the `GrantHeroXP` row of the victim's
/// level to nearby heroes, and the WC3 XP curve levels the hero, adding one
/// skill point and the STR/AGI/INT growth.
#[test]
fn hero_xp_kill_level() {
    let mut sim = Sim::new(4464, 64, 64);
    sim.passive_income = false;
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(500.0, 500.0))
        .expect("paladin");
    let victim = sim
        .spawn_def(1, "hrif", Vec2::new(560.0, 500.0))
        .expect("rifleman");
    assert_eq!(sim.unit(victim).unwrap().level, 3);
    let strength = sim.unit(hero).unwrap().hero_strength;
    let points = sim.unit(hero).unwrap().skill_points;

    sim.apply_damage(hero, victim, 100_000, AttackType::Magic);
    assert!(!sim.unit(victim).unwrap().alive);
    let grant = wc3_sim::GRANT_HERO_XP[2];
    assert_eq!(sim.unit(hero).unwrap().xp, grant, "level-3 kill XP");
    assert_eq!(sim.unit(hero).unwrap().level, 1, "one kill is not a level");

    let rest = wc3_sim::NEED_HERO_XP[0] - grant;
    sim.add_hero_xp(hero, rest).expect("xp");
    assert_eq!(sim.unit(hero).unwrap().level, 2, "NeedHeroXP curve");
    assert_eq!(sim.unit(hero).unwrap().skill_points, points + 1);
    assert!(
        sim.unit(hero).unwrap().hero_strength > strength,
        "STR growth on level-up"
    );
}
