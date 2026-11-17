//! mechanic `upgrades.execution`: all 89 shipped upgrade rows are researchable
//! through the per-race tech tree; tier gating is enforced.

use wc3_sim::{ROSTER, Race, Sim, Vec2, data, def_of};

#[test]
fn upgrades_execution_89_rows() {
    assert_eq!(data::UPGRADES.len(), 89, "89 shipped UpgradeData rows");
    let counts: Vec<(Race, usize)> = [
        (Race::Human, 21),
        (Race::Orc, 19),
        (Race::Undead, 18),
        (Race::NightElf, 22),
        (Race::Neutral, 9),
    ]
    .to_vec();
    for (race, expected) in &counts {
        assert_eq!(
            data::UPGRADES
                .iter()
                .filter(|upgrade| upgrade.race == *race)
                .count(),
            *expected,
            "{race:?} upgrade rows"
        );
    }

    let mut researched = 0usize;
    for race in [Race::Human, Race::Orc, Race::Undead, Race::NightElf] {
        let mut sim = Sim::new(0x3741 + race as u64, 96, 96);
        sim.passive_income = false;
        sim.resources[0] = 1_000_000;
        sim.lumber[0] = 1_000_000;
        let hall_def = data::main_hall_for(race).expect("hall");
        let hall = sim
            .spawn_def_exact(0, hall_def.id, Vec2::new(600.0, 600.0))
            .expect("hall");
        let mut lane = 0usize;
        let mut buildings: Vec<(&str, wc3_sim::UnitId)> = vec![(hall_def.id, hall)];
        for def in ROSTER
            .iter()
            .filter(|def| def.race == race && def.is_building())
        {
            if def.id == hall_def.id {
                continue;
            }
            let pos = Vec2::new(
                1200.0 + (lane % 6) as f32 * 420.0,
                900.0 + (lane / 6) as f32 * 420.0,
            );
            lane += 1;
            if let Some(unit) = sim.spawn_def_exact(0, def.id, pos) {
                buildings.push((def.id, unit));
            }
        }
        for upgrade in data::UPGRADES.iter().filter(|upgrade| upgrade.race == race) {
            let producer = buildings
                .iter()
                .find_map(|(id, unit)| {
                    def_of(id)
                        .filter(|def| def.researches.contains(&upgrade.id))
                        .map(|_| *unit)
                })
                .unwrap_or(hall);
            let gold_before = sim.resources[0];
            sim.research_upgrade(producer, upgrade.id)
                .unwrap_or_else(|error| panic!("{race:?} {}: {error}", upgrade.id));
            assert!(
                gold_before > sim.resources[0],
                "{race:?} {} charged no gold",
                upgrade.id
            );
            let mut done = false;
            for _ in 0..(upgrade.research_ticks(1) + 60) {
                sim.step(&[]);
                if sim.upgrade_level(0, upgrade.id) > 0 {
                    done = true;
                    break;
                }
            }
            assert!(done, "{race:?} {} never finished", upgrade.id);
            assert!(
                sim.events
                    .iter()
                    .any(|event| matches!(event, wc3_sim::Event::UpgradeResearched { id, .. } if *id == upgrade.id))
            );
            researched += 1;
        }
    }
    // The nine orphan (neutral/campaign) rows research at any main hall.
    let mut sim = Sim::new(0x3749, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 1_000_000;
    let hall = sim
        .spawn_def_exact(0, "htow", Vec2::new(500.0, 500.0))
        .expect("hall");
    for upgrade in data::UPGRADES
        .iter()
        .filter(|upgrade| upgrade.race == Race::Neutral)
    {
        sim.research_upgrade(hall, upgrade.id)
            .unwrap_or_else(|error| panic!("neutral {}: {error}", upgrade.id));
        for _ in 0..(upgrade.research_ticks(1) + 60) {
            sim.step(&[]);
        }
        assert!(sim.upgrade_level(0, upgrade.id) > 0, "{}", upgrade.id);
        researched += 1;
    }
    assert_eq!(researched, 89, "every upgrade row researched once");

    // Building tier gate: the top hall is denied before its predecessor.
    for (race, tier2, top) in [
        (Race::Human, "hkee", "hcas"),
        (Race::Orc, "ostr", "ofrt"),
        (Race::Undead, "unp1", "unp2"),
        (Race::NightElf, "etoa", "etoe"),
    ] {
        let mut sim = Sim::new(0x374A + race as u64, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 1_000_000;
        let hall_def = data::main_hall_for(race).expect("hall");
        let _ = sim
            .spawn_def_exact(0, hall_def.id, Vec2::new(400.0, 400.0))
            .expect("hall");
        let worker = sim
            .spawn_def_exact(
                0,
                match race {
                    Race::Human => "hpea",
                    Race::Orc => "opeo",
                    Race::Undead => "uaco",
                    _ => "ewsp",
                },
                Vec2::new(700.0, 400.0),
            )
            .expect("worker");
        assert_eq!(
            sim.place_building(worker, top, Vec2::new(1100.0, 400.0)),
            Err("missing requirement"),
            "{race:?}: {top} without {tier2}"
        );
        let _ = sim
            .spawn_def_exact(0, tier2, Vec2::new(1500.0, 700.0))
            .expect(tier2);
        assert!(
            sim.place_building(worker, top, Vec2::new(1100.0, 400.0))
                .is_ok(),
            "{race:?}: {top} after {tier2}"
        );
    }
}
