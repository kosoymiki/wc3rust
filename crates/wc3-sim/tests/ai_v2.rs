//! AI v2 mechanics, one test per mechanic:
//! `ai.build-order.per-race`, `ai.build-order.replan-after-loss`,
//! `ai.combat.regroup-then-attack`, `ai.combat.focus-fire`,
//! `ai.economy.saturation`, `ai.hero.casts`, `ai.expansion.safest-mine`.

mod common;

use common::*;
use wc3_sim::{AiConfig, AiMode, Event, Order, Race, SeatController, Sim, Vec2, data};

fn race_config(race: Race) -> AiConfig {
    AiConfig {
        workers: 5,
        army_supply: 4,
        first_attack_tick: 100_000,
        wave_interval: 100_000,
        retreat_permille: 250,
        composition: wc3_sim::ai_composition_for(race),
        worker: match race {
            Race::Human => "hpea",
            Race::Orc => "opeo",
            Race::Undead => "uaco",
            Race::NightElf => "ewsp",
            Race::Neutral => "hpea",
        },
    }
}

/// `ai.build-order.per-race`: every race follows the data-derived order
/// hall -> supply -> army producer -> tech/altar, and reaches a five-worker
/// crew.  Nothing in the test names a building: the expected supply/producer/
/// altar ids come from the roster tables the AI itself reads.
#[test]
fn ai_build_order_per_race() {
    for (index, race) in [Race::Human, Race::Orc, Race::Undead, Race::NightElf]
        .into_iter()
        .enumerate()
    {
        let seed = 0xB01D + index as u64;
        let mut sim = Sim::new(seed, 128, 128);
        sim.passive_income = false;
        sim.resources = [4_000, 4_000];
        let setup = sim
            .setup_melee(
                &[
                    (Race::Human, SeatController::Human),
                    (race, SeatController::Ai(race_config(race))),
                ],
                seed,
            )
            .expect("melee setup");
        let main_hall = data::main_hall_for(race).expect("main hall data").id;
        let supply = data::supply_building_for(race).expect("supply building");
        let producer = wc3_sim::ai_composition_for(race)
            .first()
            .and_then(|(unit, _)| wc3_sim::def_of(unit))
            .and_then(|def| def.trained_at.first().copied())
            .expect("producer id");
        let altar = data::altar_for(race).expect("altar data");
        let mut order: Vec<&'static str> = Vec::new();
        let mut max_workers = 0usize;
        for _ in 0..14_000 {
            sim.step(&[]);
            for unit in sim.units.iter() {
                if unit.alive
                    && unit.owner == 1
                    && unit.is_building()
                    && !order.contains(&unit.def().id)
                {
                    order.push(unit.def().id);
                }
            }
            max_workers = max_workers.max(
                sim.units
                    .iter()
                    .filter(|unit| {
                        unit.alive && unit.owner == 1 && unit.def().stats.is_worker
                    })
                    .count(),
            );
        }
        let position = |id: &str| order.iter().position(|entry| *entry == id);
        assert!(
            position(main_hall).is_some(),
            "{race:?}: no main hall; order={order:?}"
        );
        let first_non_hall = order.iter().find(|id| **id != main_hall).copied();
        assert_eq!(
            first_non_hall,
            Some(supply.id),
            "{race:?}: first building after the opening hall must be the data supply building {id}; order={order:?}",
            id = supply.id
        );
        let supply_at = position(supply.id).expect("supply placed");
        let producer_at = position(producer).unwrap_or_else(|| {
            panic!("{race:?}: producer {producer} missing; order={order:?}")
        });
        assert!(
            producer_at > supply_at,
            "{race:?}: producer {producer} must follow supply; order={order:?}"
        );
        let altar_at = position(altar.id).unwrap_or_else(|| {
            panic!("{race:?}: altar {} missing; order={order:?}", altar.id)
        });
        assert!(
            altar_at > producer_at,
            "{race:?}: altar {} must follow the producer; order={order:?}",
            altar.id
        );
        assert!(
            max_workers >= 5,
            "{race:?}: worker boom reached {max_workers}, expected 5"
        );
        eprintln!("{race:?} build order: {order:?}");
        let _ = setup;
    }
}

/// `ai.build-order.replan-after-loss`: the plan is recomputed from live state,
/// so a destroyed barracks (and even the main hall) is rebuilt without any
/// stored "already built" flag.
#[test]
fn ai_build_order_replans_after_loss() {
    let mut sim = Sim::new(0xB02D, 96, 96);
    sim.passive_income = false;
    sim.resources = [30_000, 30_000];
    sim.lumber = [5_000, 5_000];
    hall(&mut sim, 0, Vec2::new(8000.0, 8000.0));
    hall(&mut sim, 1, Vec2::new(800.0, 800.0));
    sim.add_gold_mine(Vec2::new(1500.0, 800.0), 20_000);
    for index in 0..5 {
        worker(&mut sim, 1, Vec2::new(900.0 + index as f32 * 30.0, 900.0));
    }
    sim.step(&[]);
    sim.enable_ai(
        1,
        AiConfig {
            workers: 5,
            army_supply: 6,
            first_attack_tick: 100_000,
            wave_interval: 100_000,
            retreat_permille: 250,
            composition: &[("hfoo", 1)],
            worker: "hpea",
        },
    );
    let mut barracks = None;
    for _ in 0..6_000 {
        sim.step(&[]);
        if let Some(unit) = sim
            .units_of(1)
            .find(|unit| unit.alive && unit.def().id == "hbar" && unit.construct.is_none())
        {
            barracks = Some(unit.id);
            break;
        }
    }
    let old = barracks.expect("AI built a barracks before the loss test");
    sim.kill_unit(old);
    let mut rebuilt = None;
    for _ in 0..2_400 {
        sim.step(&[]);
        if let Some(unit) = sim.units_of(1).find(|unit| {
            unit.alive && unit.def().id == "hbar" && unit.id != old
        }) {
            rebuilt = Some(unit.id);
            break;
        }
    }
    assert!(
        rebuilt.is_some(),
        "AI re-planned the destroyed barracks (old {:?})",
        old
    );

    // Main hall loss: the AI used to freeze because `first_hall` went `None`.
    let old_hall = sim
        .units_of(1)
        .find(|unit| unit.alive && unit.def().is_main_hall())
        .map(|unit| unit.id)
        .expect("AI hall");
    sim.kill_unit(old_hall);
    let mut hall_back = false;
    for _ in 0..6_000 {
        sim.step(&[]);
        if sim.units_of(1).any(|unit| {
            unit.alive && unit.def().is_main_hall() && unit.id != old_hall
        }) {
            hall_back = true;
            break;
        }
    }
    assert!(hall_back, "AI rebuilt its main hall after destruction");
}

/// `ai.combat.regroup-then-attack`: a wave first gathers at a rally point on
/// the road to the enemy base and only then attack-moves as a group.
#[test]
fn ai_combat_regroup_then_attack() {
    let mut sim = Sim::new(0xC0B1, 96, 96);
    sim.passive_income = false;
    sim.resources = [20_000, 20_000];
    hall(&mut sim, 0, Vec2::new(8000.0, 8000.0));
    hall(&mut sim, 1, Vec2::new(800.0, 800.0));
    sim.add_gold_mine(Vec2::new(1500.0, 800.0), 20_000);
    let army: Vec<_> = (0..3)
        .map(|index| footman(&mut sim, 1, Vec2::new(830.0 + index as f32 * 25.0, 830.0)))
        .collect();
    sim.step(&[]);
    sim.enable_ai(
        1,
        AiConfig {
            workers: 1,
            army_supply: 1,
            first_attack_tick: 0,
            wave_interval: 100_000,
            retreat_permille: 0,
            composition: &[("hfoo", 1)],
            worker: "hpea",
        },
    );
    // The rally point is on the hall -> enemy-base line, 600 from the hall.
    let hall_pos = Vec2::new(800.0, 800.0);
    let enemy = Vec2::new(8000.0, 8000.0);
    let direction = (enemy - hall_pos).normalize();
    let expected_rally = hall_pos + direction * 600.0;
    let mut moved_to_rally = false;
    for _ in 0..240 {
        sim.step(&[]);
        if sim.ai_waves(1) == 0
            && army.iter().any(|id| {
                sim.unit(*id)
                    .and_then(|unit| unit.goal)
                    .map(|goal| goal.distance(expected_rally) < 120.0)
                    .unwrap_or(false)
            })
        {
            moved_to_rally = true;
            break;
        }
    }
    assert!(
        moved_to_rally,
        "wave gathered at the rally point before attacking"
    );
    assert_eq!(sim.ai_waves(1), 0, "no wave is counted during the regroup");

    let mut attacked = false;
    for _ in 0..1_200 {
        sim.step(&[]);
        if sim.ai_waves(1) >= 1 {
            attacked = true;
            break;
        }
    }
    assert!(attacked, "wave left the rally and attack-moved");
    assert!(
        sim.ai
            .iter()
            .any(|state| state.owner == 1 && state.mode == AiMode::Attack),
        "AI switched to Attack after the regroup"
    );
    assert!(
        army.iter().any(|id| {
            sim.unit(*id)
                .and_then(|unit| unit.goal)
                .map(|goal| goal.distance(enemy) < 5_000.0)
                .unwrap_or(false)
        }),
        "attack-move goal is the enemy base, not the rally"
    );
}

/// `ai.combat.focus-fire`: in Attack mode the army switches to the lowest-HP
/// enemy near the fight instead of each unit picking its own target.
#[test]
fn ai_combat_focus_fire() {
    let mut sim = Sim::new(0xF0C5, 96, 96);
    sim.passive_income = false;
    sim.resources = [20_000, 20_000];
    hall(&mut sim, 0, Vec2::new(8000.0, 8000.0));
    hall(&mut sim, 1, Vec2::new(800.0, 800.0));
    let army: Vec<_> = (0..3)
        .map(|index| footman(&mut sim, 1, Vec2::new(1700.0 + index as f32 * 40.0, 1700.0)))
        .collect();
    let strong = footman(&mut sim, 0, Vec2::new(2000.0, 1700.0));
    let weak = footman(&mut sim, 0, Vec2::new(2250.0, 1700.0));
    if let Some(unit) = sim.unit_mut(weak) {
        unit.hp = 40;
    }
    sim.step(&[]);
    sim.enable_ai(
        1,
        AiConfig {
            workers: 1,
            army_supply: 1,
            first_attack_tick: 100_000,
            wave_interval: 100_000,
            retreat_permille: 0,
            composition: &[("hfoo", 1)],
            worker: "hpea",
        },
    );
    if let Some(state) = sim.ai.iter_mut().find(|state| state.owner == 1) {
        state.mode = AiMode::Attack;
        state.next_wave = 1_000_000;
    }
    let mut focused = false;
    for _ in 0..600 {
        sim.step(&[]);
        if army.iter().any(|id| {
            sim.unit(*id)
                .map(|unit| matches!(unit.order, Order::Attack(target) if target == weak))
                .unwrap_or(false)
        }) {
            focused = true;
            break;
        }
        if !sim.unit(weak).map(|unit| unit.alive).unwrap_or(false) {
            break;
        }
    }
    assert!(
        focused,
        "AI focus-fired the 40-HP footman instead of the healthy one ({:?})",
        strong
    );
}

/// `ai.economy.saturation`: five workers per mine, a lumber crew on top of the
/// configured crew, lost workers retrained, and damaged buildings repaired.
#[test]
fn ai_economy_saturation() {
    let mut sim = Sim::new(0xE0C0, 96, 96);
    sim.passive_income = false;
    sim.resources = [4_000, 4_000];
    sim.lumber = [2_000, 2_000];
    let hall_pos = Vec2::new(2400.0, 2400.0);
    hall(&mut sim, 0, hall_pos);
    let mine = Vec2::new(3000.0, 2400.0);
    sim.add_gold_mine(mine, 50_000);
    for index in 0..24 {
        let angle = index as f32 * std::f32::consts::TAU / 24.0;
        sim.spawn_destructable(
            "LTlt",
            hall_pos + Vec2::new(angle.cos(), angle.sin()) * 520.0,
        );
    }
    for index in 0..12 {
        let angle = index as f32 * std::f32::consts::TAU / 12.0;
        worker(
            &mut sim,
            0,
            hall_pos + Vec2::new(angle.cos(), angle.sin()) * 260.0,
        );
    }
    // A damaged farm exercises the repair step.
    let farm = sim
        .spawn_def(0, "hhou", Vec2::new(1900.0, 2400.0))
        .expect("farm");
    if let Some(unit) = sim.unit_mut(farm) {
        unit.hp = unit.max_hp / 2;
    }
    sim.step(&[]);
    let mut config = AiConfig::for_race(Race::Human);
    config.workers = 12;
    config.first_attack_tick = 100_000;
    config.wave_interval = 100_000;
    sim.enable_ai(0, config);

    let crew = |sim: &Sim, order: fn(&Order) -> bool| {
        sim.units
            .iter()
            .filter(|unit| {
                unit.alive && unit.owner == 0 && unit.def().stats.is_worker && order(&unit.order)
            })
            .count()
    };
    let mut max_gold = 0usize;
    let mut max_lumber = 0usize;
    let farm_hp0 = sim.unit(farm).unwrap().hp;
    let mut farm_hp = farm_hp0;
    for tick in 0..2_400u32 {
        sim.step(&[]);
        max_gold = max_gold.max(crew(&sim, |order| {
            matches!(order, Order::Harvest { .. })
        }));
        max_lumber = max_lumber.max(crew(&sim, |order| {
            matches!(order, Order::HarvestLumber { .. })
        }));
        if tick > 600 {
            farm_hp = farm_hp.max(sim.unit(farm).map(|unit| unit.hp).unwrap_or(0));
        }
    }
    assert_eq!(
        max_gold, wc3_sim::ai::MINE_SATURATION,
        "one mine keeps exactly {} workers, saw {max_gold}",
        wc3_sim::ai::MINE_SATURATION
    );
    assert!(
        max_lumber >= 2,
        "a lumber crew works on top of the gold line, saw {max_lumber}"
    );
    assert!(
        sim.units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 0 && unit.def().stats.is_worker)
            .count()
            > 12,
        "the AI trains extra lumberjacks above the configured gold crew"
    );
    assert!(
        farm_hp > farm_hp0,
        "damaged farm repaired: {farm_hp0} -> {farm_hp}"
    );

    // Lose three workers: the same crew target must rebuild them.
    let victims: Vec<_> = sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == 0 && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .take(3)
        .collect();
    for id in victims {
        sim.kill_unit(id);
    }
    let mut rebuilt = false;
    for _ in 0..2_400 {
        sim.step(&[]);
        let workers = sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 0 && unit.def().stats.is_worker)
            .count();
        if workers >= 13 {
            rebuilt = true;
            break;
        }
    }
    assert!(rebuilt, "AI rebuilt its lost workers back to the crew target");
}

/// `ai.hero.casts`: a hero in the AI army uses its spell through the regular
/// `Sim::cast` path (event evidence, not a private field).
#[test]
fn ai_hero_casts() {
    let mut sim = Sim::new(0xAE20, 64, 64);
    sim.passive_income = false;
    sim.resources = [20_000, 20_000];
    sim.lumber = [20_000, 20_000];
    hall(&mut sim, 0, Vec2::new(7000.0, 7000.0));
    hall(&mut sim, 1, Vec2::new(800.0, 800.0));
    sim.spawn_def(1, "halt", Vec2::new(1200.0, 800.0))
        .expect("altar");
    let paladin = sim
        .spawn_def(1, "Hpal", Vec2::new(900.0, 900.0))
        .expect("paladin");
    let hurt = footman(&mut sim, 1, Vec2::new(950.0, 900.0));
    if let Some(unit) = sim.unit_mut(hurt) {
        unit.hp = 100;
    }
    sim.step(&[]);
    sim.enable_ai(
        1,
        AiConfig {
            workers: 1,
            army_supply: 1,
            first_attack_tick: 100_000,
            wave_interval: 100_000,
            retreat_permille: 250,
            composition: &[("hfoo", 1)],
            worker: "hpea",
        },
    );
    let mut healed = false;
    for _ in 0..300 {
        sim.step(&[]);
        if sim.events.iter().any(|event| {
            matches!(
                event,
                Event::SpellHeal { caster, amount, .. } if *caster == paladin && *amount > 0
            )
        }) {
            healed = true;
            break;
        }
    }
    assert!(healed, "AI hero cast Holy Light through Sim::cast");
}

/// `ai.expansion.safest-mine`: the expansion picks the mine that is safe from
/// the enemy, not merely the closest one.
#[test]
fn ai_expansion_safest_mine() {
    let mut sim = Sim::new(0xE5AF, 96, 96);
    sim.passive_income = false;
    sim.resources = [30_000, 30_000];
    sim.lumber = [5_000, 5_000];
    hall(&mut sim, 0, Vec2::new(8000.0, 8000.0));
    hall(&mut sim, 1, Vec2::new(800.0, 800.0));
    sim.add_gold_mine(Vec2::new(1500.0, 800.0), 20_000);
    let unsafe_mine = Vec2::new(2500.0, 800.0);
    let safe_mine = Vec2::new(5600.0, 800.0);
    sim.add_gold_mine(unsafe_mine, 20_000);
    sim.add_gold_mine(safe_mine, 20_000);
    // Enemy pickets sit on the near mine; the far one is quiet.
    footman(&mut sim, 0, Vec2::new(2550.0, 850.0));
    footman(&mut sim, 0, Vec2::new(2600.0, 900.0));
    for index in 0..3 {
        worker(&mut sim, 1, Vec2::new(900.0 + index as f32 * 40.0, 950.0));
    }
    sim.step(&[]);
    sim.enable_ai(
        1,
        AiConfig {
            workers: 3,
            army_supply: 6,
            first_attack_tick: 100_000,
            wave_interval: 100_000,
            retreat_permille: 250,
            composition: &[("hfoo", 1)],
            worker: "hpea",
        },
    );
    let main_hall_pos = sim
        .unit(sim.main_hall(1).expect("AI main hall"))
        .expect("AI main hall unit")
        .pos;
    let mut expansion = None;
    for _ in 0..14_000 {
        sim.step(&[]);
        let mut halls: Vec<Vec2> = sim
            .units_of(1)
            .filter(|unit| unit.alive && unit.def().is_main_hall())
            .map(|unit| unit.pos)
            .collect();
        if halls.len() >= 2 {
            halls.retain(|pos| pos.distance(main_hall_pos) > 1.0);
            expansion = halls.into_iter().max_by(|a, b| {
                a.distance(main_hall_pos)
                    .partial_cmp(&b.distance(main_hall_pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            break;
        }
    }
    let site = expansion.expect("AI expanded to a second hall");
    assert!(
        site.distance(safe_mine) < site.distance(unsafe_mine),
        "expansion at ({:.0},{:.0}) is closer to the safe mine than the picketed one",
        site.x,
        site.y
    );
    assert!(
        site.distance(safe_mine) < 1200.0,
        "expansion sits at the safe mine ({:.0},{:.0}), not {:.0} away",
        safe_mine.x,
        safe_mine.y,
        site.distance(safe_mine)
    );
}
