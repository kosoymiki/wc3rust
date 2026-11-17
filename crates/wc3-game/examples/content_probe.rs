//! Content census: every race's buildings, the worker build menu, upgrades.
//!
//! The operator's complaint is content, not pixels: «постройки все, всем расам,
//! меню построек, все улучшения зданий, все бафы, лесопилка, каждое здание».
//! This prints exactly what the data has, so gaps are numbers, not opinions.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example content_probe --offline
//! ```

use wc3_sim::data::{self, Race};

const RACES: [Race; 4] = [Race::Human, Race::Orc, Race::Undead, Race::NightElf];

fn race_name(race: Race) -> &'static str {
    match race {
        Race::Human => "Human",
        Race::Orc => "Orc",
        Race::Undead => "Undead",
        Race::NightElf => "NightElf",
        Race::Neutral => "Neutral",
    }
}

fn main() {
    let mut roster_buildings = 0usize;
    let mut all_buildings = 0usize;
    let mut roster_models = 0usize;
    let mut all_models = 0usize;
    for race in RACES {
        let buildings = data::building_defs(race);
        let roster: Vec<&data::UnitDef> = data::ROSTER
            .iter()
            .filter(|def| def.race == race && def.is_building())
            .collect();
        let buildable: Vec<&data::UnitDef> = buildings.iter().copied().filter(|d| d.buildable).collect();
        let worker: Vec<&data::UnitDef> = data::worker_buildable(race);
        let with_model = buildings.iter().filter(|d| d.model_present).count();
        let roster_with_model = roster.iter().filter(|d| d.model_present).count();
        println!(
            "RACE {:<8} buildings={} (roster {}) buildable={} worker_buildable={} model_present={}/{} roster_models={}/{}",
            race_name(race),
            buildings.len(),
            roster.len(),
            buildable.len(),
            worker.len(),
            with_model,
            buildings.len(),
            roster_with_model,
            roster.len()
        );
        let menu = data::build_menu(race);
        let menu_ids: Vec<&str> = menu.iter().map(|d| d.id).collect();
        println!(
            "  build_menu entries={} ids=[{}]",
            menu.len(),
            menu_ids.join(" ")
        );
        let missing: Vec<&str> = worker
            .iter()
            .filter(|def| !menu.iter().any(|entry| entry.id == def.id))
            .map(|def| def.id)
            .collect();
        let extra: Vec<&str> = menu
            .iter()
            .filter(|def| !worker.iter().any(|entry| entry.id == def.id))
            .map(|def| def.id)
            .collect();
        println!(
            "  menu_check missing_from_menu=[{}] extra_in_menu=[{}] ok={}",
            missing.join(" "),
            extra.join(" "),
            missing.is_empty() && extra.is_empty()
        );
        let unreachable: Vec<&str> = buildings
            .iter()
            .filter(|def| {
                !data::is_worker_buildable(def)
                    && data::unit_upgrade_of(def.id).is_none()
                    && data::unit_placed_by_ability(def.id).is_none()
            })
            .map(|def| def.id)
            .collect();
        let upgrade_reachable: Vec<String> = buildings
            .iter()
            .filter_map(|def| {
                let from = data::unit_upgrade_of(def.id)?;
                Some(format!("{}<-{}", def.id, from))
            })
            .collect();
        let ability_reachable: Vec<String> = buildings
            .iter()
            .filter_map(|def| {
                let ability = data::unit_placed_by_ability(def.id)?;
                Some(format!("{}<-{}", def.id, ability))
            })
            .collect();
        println!(
            "  reach: unreachable=[{}] upgrade_paths=[{}] ability_placed=[{}]",
            unreachable.join(" "),
            upgrade_reachable.join(" "),
            ability_reachable.join(" ")
        );
        for def in &buildings {
            println!(
                "  bld {:<6} {:<28} fp={} cost={:>4}/{:<3} buildable={:<5} model={:<5} req=[{}] trains={} researches={}",
                def.id,
                def.name,
                def.footprint,
                def.stats.cost,
                def.stats.lumber_cost,
                def.buildable,
                def.model_present,
                def.requires.join(" "),
                def.trains.len(),
                def.researches.len()
            );
        }
        for producer in buildings.iter().filter(|def| !def.researches.is_empty()) {
            println!(
                "  upg-producer {:<6} researches={:?}",
                producer.id, producer.researches
            );
        }
        roster_buildings += roster.len();
        all_buildings += buildings.len();
        roster_models += roster_with_model;
        all_models += with_model;
    }

    // Trainable units: the `Trains=` card of every building of the race must
    // resolve to a def, and every trainable def must point at a real model.
    // Ledger 472: before the TFT units landed this printed 54/63 with nine
    // missing ids (hspt hdhw hrtt otbk otbr ospm uobs emtg efdr).
    let mut units_total = 0usize;
    let mut units_present = 0usize;
    let mut unit_models = 0usize;
    let mut unit_models_missing = Vec::<String>::new();
    let mut missing_all = Vec::<String>::new();
    for race in RACES {
        let mut trained: Vec<&str> = Vec::new();
        let mut missing = Vec::<&str>::new();
        for def in data::ROSTER
            .iter()
            .filter(|def| def.race == race && def.is_building())
        {
            for unit_id in def.trains {
                if !trained.contains(unit_id) {
                    trained.push(unit_id);
                }
                if data::def_of(unit_id).is_none() && !missing.contains(unit_id) {
                    missing.push(unit_id);
                }
            }
        }
        let trainable: Vec<&data::UnitDef> = data::ROSTER
            .iter()
            .filter(|def| def.race == race && !def.is_building() && !def.trained_at.is_empty())
            .collect();
        let models = trainable.iter().filter(|def| def.model_present).count();
        println!(
            "UNITS {:<8} trains_total={} trains_present={} missing=[{}] trainable={} models={}/{}",
            race_name(race),
            trained.len(),
            trained.len() - missing.len(),
            missing.join(" "),
            trainable.len(),
            models,
            trainable.len()
        );
        for def in &trainable {
            if !def.model_present {
                unit_models_missing.push(format!("{}: {}", def.id, def.model));
            }
            let abilities: Vec<&str> = def.ability_ids.to_vec();
            let upgrades = data::unit_upgrades(def.id);
            println!(
                "  unit {:<6} {:<22} hp={:<5} cost={:>4}/{:<3} supply={:<2} lvl={:<2} model={:<5} icon={:<5} trained_at=[{}] requires=[{}] abilities={} upgrades={}",
                def.id,
                def.name,
                def.stats.hp,
                def.stats.cost,
                def.stats.lumber_cost,
                def.stats.supply,
                def.stats.level,
                def.model_present,
                !def.icon.is_empty(),
                def.trained_at.join(" "),
                def.requires.join(" "),
                abilities.len(),
                upgrades.len()
            );
        }
        units_total += trained.len();
        units_present += trained.len() - missing.len();
        unit_models += models;
        for unit_id in missing {
            missing_all.push(format!("{}:{}", race_name(race), unit_id));
        }
    }
    let tavern = data::tavern().expect("tavern");
    println!(
        "TAVERN {:<6} trains_total={} trains_present={} ids=[{}]",
        tavern.id,
        tavern.trains.len(),
        tavern.trains.iter().filter(|id| data::def_of(id).is_some()).count(),
        tavern.trains.join(" ")
    );
    println!(
        "TOTALS_UNITS trains_total={} trains_present={} missing={} ids=[{}] trainable={} models_present={}/{} models_missing=[{}]",
        units_total,
        units_present,
        missing_all.len(),
        missing_all.join(" "),
        unit_models,
        unit_models,
        unit_models,
        unit_models_missing.join(", ")
    );

    let mut four_race_upgrades = 0usize;
    let mut zero_effect = Vec::<&str>::new();
    let mut data_only = Vec::<(&str, &str, &str)>::new();
    let mut applied_effects = 0usize;
    for race in RACES {
        for upgrade in data::UPGRADES.iter().filter(|upgrade| upgrade.race == race) {
            four_race_upgrades += 1;
            if upgrade.effects.is_empty() {
                zero_effect.push(upgrade.id);
            }
            for effect in upgrade.effects {
                if effect.kind == data::UpgradeEffectKind::DataOnly {
                    data_only.push((upgrade.id, upgrade.name, effect.note));
                } else {
                    applied_effects += 1;
                }
            }
        }
    }
    println!(
        "UPGRADES four_race={} all={} zero_effect={} applied_effects={} data_only_effects={}",
        four_race_upgrades,
        data::UPGRADES.len(),
        zero_effect.len(),
        applied_effects,
        data_only.len()
    );
    if !zero_effect.is_empty() {
        println!("  zero_effect ids=[{}]", zero_effect.join(" "));
    }
    for (id, name, note) in &data_only {
        println!("  data-only {:<8} {:<26} reason={}", id, name, note);
    }
    let mut ability_grants = 0usize;
    for upgrade in data::UPGRADES.iter() {
        for effect in upgrade.effects {
            if effect.kind == data::UpgradeEffectKind::UnlockAbility {
                ability_grants += 1;
                println!(
                    "  unlock {:<8} -> {:<6} ({})",
                    upgrade.id, effect.ability, upgrade.name
                );
            }
        }
    }
    println!("  unlock_ability_grants={ability_grants}");

    println!(
        "TOTALS roster_buildings={} roster_models={}/{} all_buildings={} all_models={}/{}",
        roster_buildings,
        roster_models,
        roster_buildings,
        all_buildings,
        all_models,
        all_buildings
    );
}
