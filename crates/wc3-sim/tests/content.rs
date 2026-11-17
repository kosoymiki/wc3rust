//! Content-completeness mechanics, one test per mechanic id:
//!
//! ```text
//! content::buildings::all_reachable
//! content::upgrades::all_have_effects
//! content::research::applies_to_stats
//! content::models::all_present
//! content::lumber_mill::research_menu
//! ```

use wc3_sim::data::{self, Race};
use wc3_sim::{AttackType, Event, Sim, Vec2};

const RACES: [Race; 4] = [Race::Human, Race::Orc, Race::Undead, Race::NightElf];

mod census {
    use super::*;

    /// Prints the same table as `wc3-game --example content_probe` so the
    /// census is reproducible from the sim crate alone (`-- --nocapture`).
    #[test]
    fn prints_table() {
        let (mut roster, mut all) = (0usize, 0usize);
        for race in RACES {
            let buildings = data::building_defs(race);
            let menu = data::build_menu(race);
            let worker = data::worker_buildable(race);
            let roster_buildings: Vec<&data::UnitDef> = data::ROSTER
                .iter()
                .filter(|def| def.race == race && def.is_building())
                .collect();
            let models = buildings.iter().filter(|def| def.model_present).count();
            let roster_models = roster_buildings
                .iter()
                .filter(|def| def.model_present)
                .count();
            println!(
                "RACE {:<8} buildings={} (roster {}) buildable={} worker_buildable={} menu={} model_present={}/{} roster_models={}/{}",
                format!("{race:?}"),
                buildings.len(),
                roster_buildings.len(),
                buildings.iter().filter(|d| d.buildable).count(),
                worker.len(),
                menu.len(),
                models,
                buildings.len(),
                roster_models,
                roster_buildings.len()
            );
            println!(
                "  menu ids=[{}]",
                menu.iter().map(|d| d.id).collect::<Vec<_>>().join(" ")
            );
            let missing: Vec<&str> = worker
                .iter()
                .filter(|d| !menu.iter().any(|m| m.id == d.id))
                .map(|d| d.id)
                .collect();
            let extra: Vec<&str> = menu
                .iter()
                .filter(|d| !worker.iter().any(|m| m.id == d.id))
                .map(|d| d.id)
                .collect();
            println!(
                "  menu_check missing=[{}] extra=[{}] ok={}",
                missing.join(" "),
                extra.join(" "),
                missing.is_empty() && extra.is_empty()
            );
            let paths: Vec<String> = buildings
                .iter()
                .filter_map(|d| {
                    let from = data::unit_upgrade_of(d.id)?;
                    Some(format!("{}<-{}", d.id, from))
                })
                .collect();
            let placed: Vec<String> = buildings
                .iter()
                .filter_map(|d| {
                    let ability = data::unit_placed_by_ability(d.id)?;
                    Some(format!("{}<-{}", d.id, ability))
                })
                .collect();
            println!(
                "  reach: unreachable=[{}] upgrade_paths=[{}] ability_placed=[{}]",
                buildings
                    .iter()
                    .filter(|d| !data::is_worker_buildable(d)
                        && data::unit_upgrade_of(d.id).is_none()
                        && data::unit_placed_by_ability(d.id).is_none())
                    .map(|d| d.id)
                    .collect::<Vec<_>>()
                    .join(" "),
                paths.join(" "),
                placed.join(" ")
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
            for producer in buildings.iter().filter(|d| !d.researches.is_empty()) {
                println!(
                    "  upg-producer {:<6} researches={:?}",
                    producer.id, producer.researches
                );
            }
            roster += roster_buildings.len();
            all += buildings.len();
        }
        let mut zero = 0usize;
        let mut data_only = 0usize;
        let mut applied = 0usize;
        let mut unlocks = 0usize;
        for upgrade in data::UPGRADES {
            if upgrade.effects.is_empty() {
                zero += 1;
            }
            for effect in upgrade.effects {
                match effect.kind {
                    data::UpgradeEffectKind::DataOnly => {
                        data_only += 1;
                        println!(
                            "  data-only {:<8} {:<26} reason={}",
                            upgrade.id, upgrade.name, effect.note
                        );
                    }
                    data::UpgradeEffectKind::UnlockAbility => {
                        unlocks += 1;
                        applied += 1;
                        println!(
                            "  unlock {:<8} -> {:<6} ({})",
                            upgrade.id, effect.ability, upgrade.name
                        );
                    }
                    _ => applied += 1,
                }
            }
        }
        println!(
            "UPGRADES all={} zero_effect={} applied_effects={} data_only_effects={} unlock_ability_grants={}",
            data::UPGRADES.len(),
            zero,
            applied,
            data_only,
            unlocks
        );
        println!(
            "TOTALS roster_buildings={roster} roster_models={roster}/{roster} all_buildings={all} all_models={all}/{all}"
        );
        assert_eq!(zero, 0, "no upgrade may have an empty effect list");
        assert_eq!(roster, 50);
    }
}


fn assert_same_set(left: &[&str], right: &[&str], context: &str) {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_unstable();
    right.sort_unstable();
    assert_eq!(left, right, "{context}");
}

mod buildings {
    use super::*;

    /// `content::buildings::all_reachable`: the worker build menu is exactly
    /// the set of worker-placeable buildings, and every other building def is
    /// reachable through an explicit upgrade path or a placement ability.
    #[test]
    fn all_reachable() {
        for race in RACES {
            let menu: Vec<&str> = data::build_menu(race).iter().map(|def| def.id).collect();
            let placeable: Vec<&str> = data::worker_buildable(race)
                .iter()
                .map(|def| def.id)
                .collect();
            assert_same_set(
                &menu,
                &placeable,
                &format!("{race:?} build_menu must equal worker-buildable set"),
            );
            for def in data::build_menu(race) {
                assert!(
                    data::is_worker_buildable(def),
                    "{race:?} menu entry {} is not placeable",
                    def.id
                );
            }
            for def in data::building_defs(race) {
                let reachable = data::is_worker_buildable(def)
                    || data::unit_upgrade_of(def.id).is_some()
                    || data::unit_placed_by_ability(def.id).is_some();
                assert!(
                    reachable,
                    "{race:?} building {} is in no menu, no upgrade path, no ability",
                    def.id
                );
            }
        }
        // The specific orphan from the census: `uzg1` is produced by upgrading
        // a Ziggurat (`[uzig] Upgrade=uzg1,uzg2` in TFT UndeadUnitFunc.txt).
        assert_eq!(
            data::unit_upgrade_of("uzg1"),
            Some("uzig"),
            "uzg1 must be reachable by upgrading a Ziggurat"
        );
        // Hall tiers and the tower line are upgrade paths, not worker cards.
        for (tier, from) in [
            ("hkee", "htow"),
            ("hcas", "hkee"),
            ("ostr", "ogre"),
            ("ofrt", "ostr"),
            ("unp1", "unpl"),
            ("unp2", "unp1"),
            ("etoa", "etol"),
            ("etoe", "etoa"),
            ("hgtw", "hwtw"),
            ("hctw", "hwtw"),
        ] {
            assert_eq!(data::unit_upgrade_of(tier), Some(from), "{tier} upgrade path");
        }
        assert_eq!(data::unit_placed_by_ability("egol"), Some("Aent"));
    }
}

mod upgrades {
    use super::*;

    /// `content::upgrades::all_have_effects`: every shipped upgrade carries at
    /// least one effect; ability grants name a real `AbilityData.slk` ability;
    /// the only non-working rows are the explicit `DataOnly` list.
    #[test]
    fn all_have_effects() {
        let mut zero = Vec::new();
        let mut data_only = Vec::new();
        let mut unlocks = 0;
        for upgrade in data::UPGRADES {
            if upgrade.effects.is_empty() {
                zero.push(upgrade.id);
            }
            for effect in upgrade.effects {
                if effect.kind == data::UpgradeEffectKind::DataOnly {
                    assert!(
                        !effect.note.is_empty(),
                        "{} data-only effect needs a reason",
                        upgrade.id
                    );
                    if matches!(
                        upgrade.race,
                        Race::Human | Race::Orc | Race::Undead | Race::NightElf
                    ) {
                        data_only.push(upgrade.id);
                    }
                }
                if effect.kind == data::UpgradeEffectKind::UnlockAbility {
                    unlocks += 1;
                    assert!(
                        !effect.ability.is_empty(),
                        "{} unlock effect needs an ability id",
                        upgrade.id
                    );
                    assert!(
                        wc3_sim::abilities::generic_ability(effect.ability).is_some(),
                        "{} unlocks {}, absent from AbilityData",
                        upgrade.id,
                        effect.ability
                    );
                }
            }
        }
        assert!(zero.is_empty(), "upgrades with no effect: {zero:?}");
        assert!(unlocks >= 30, "ability grants found: {unlocks}");
        let mut distinct: Vec<&str> = data_only.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct,
            vec![
                "Recb", "Remg", "Repb", "Rhgb", "Rhhb", "Rhrt", "Robf", "Robk", "Rusl", "Rusm"
            ],
            "four-race data-only upgrades must stay the documented engine-flag set"
        );
        // The four zero-effect families from the census are now real grants.
        for (upgrade, ability) in [
            ("Rhde", "Adef"),
            ("Rhss", "Acmg"),
            ("Rhpm", "AInv"),
            ("Rhfc", "Aflk"),
            ("Rhfs", "Afsh"),
            ("Rhcd", "Aclf"),
            ("Ropg", "Asal"),
            ("Ruac", "Acan"),
        ] {
            let def = data::upgrade_def(upgrade).expect(upgrade);
            assert!(
                def.effects
                    .iter()
                    .any(|effect| effect.ability == ability),
                "{upgrade} must unlock {ability}"
            );
        }
        // Ability gating: a footman's `Adef` is listed but locked by `Rhde`.
        let footman = data::def_of("hfoo").unwrap();
        assert!(footman.ability_ids.contains(&"Adef"));
        assert!(data::affected_by(footman, "Rhde"));
        assert_eq!(data::ability_unlock_upgrade("Adef"), Some("Rhde"));
        // End to end: research the grant and the unit's ability unlocks; a
        // grant the unit never lists statically (gyrocopter flak cannon) also
        // becomes available from its own `upgrades` entry.
        let mut sim = Sim::new(0x4661, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 1_000_000;
        let barracks = sim
            .spawn_def_exact(0, "hbar", Vec2::new(512.0, 512.0))
            .expect("barracks");
        let workshop = sim
            .spawn_def_exact(0, "harm", Vec2::new(900.0, 512.0))
            .expect("workshop");
        let footman = sim
            .spawn_def_exact(0, "hfoo", Vec2::new(640.0, 512.0))
            .expect("footman");
        let gyro = sim
            .spawn_def_exact(0, "hgyr", Vec2::new(1000.0, 512.0))
            .expect("gyrocopter");
        sim.step(&[]);
        assert!(!sim.unit_ability_unlocked(footman, "Adef"));
        assert!(!sim.unit_ability_unlocked(gyro, "Aflk"));
        for (building, upgrade) in [(barracks, "Rhde"), (workshop, "Rhfc")] {
            sim.research_upgrade(building, upgrade)
                .unwrap_or_else(|error| panic!("{upgrade}: {error}"));
            for _ in 0..(data::upgrade_def(upgrade).unwrap().research_ticks(1) + 30) {
                sim.step(&[]);
                if sim.upgrade_level(0, upgrade) > 0 {
                    break;
                }
            }
            assert_eq!(sim.upgrade_level(0, upgrade), 1, "{upgrade} not researched");
        }
        assert!(sim.unit_ability_unlocked(footman, "Adef"));
        assert!(
            sim.unit_ability_unlocked(gyro, "Aflk"),
            "Rhfc must grant Flak Cannon to the gyrocopter it lists"
        );
    }
}

mod research {
    use super::*;

    /// `content::research::applies_to_stats`: researching `Rhar` raises the
    /// footman's armor by the table `defUp` (2) and lowers incoming combat
    /// damage by the real armor formula.
    #[test]
    fn applies_to_stats() {
        let mut sim = Sim::new(0x4660, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        let blacksmith = sim
            .spawn_def_exact(0, "hbla", Vec2::new(512.0, 512.0))
            .expect("blacksmith");
        let footman = sim
            .spawn_def_exact(0, "hfoo", Vec2::new(640.0, 512.0))
            .expect("footman");
        sim.step(&[]);
        let before = sim.unit(footman).unwrap().armor;
        assert_eq!(before, data::def_of("hfoo").unwrap().stats.armor);
        let damage_before = data::typed_damage(AttackType::Normal, data::ArmorType::Medium, before, 30);
        let gold_before = sim.resources[0];
        sim.research_upgrade(blacksmith, "Rhar")
            .expect("blacksmith researches human armor");
        assert!(sim.resources[0] < gold_before, "Rhar charged no gold");
        let ticks = data::upgrade_def("Rhar").unwrap().research_ticks(1) + 30;
        for _ in 0..ticks {
            sim.step(&[]);
            if sim.upgrade_level(0, "Rhar") > 0 {
                break;
            }
        }
        assert_eq!(sim.upgrade_level(0, "Rhar"), 1);
        assert!(sim.events.iter().any(|event| matches!(
            event,
            Event::UpgradeResearched { id, .. } if *id == "Rhar"
        )));
        let after = sim.unit(footman).unwrap().armor;
        let expected = before + data::unit_armor_upgrade("hfoo");
        assert_eq!(after, expected, "footman armor after Rhar");
        let damage_after = data::typed_damage(AttackType::Normal, data::ArmorType::Medium, after, 30);
        assert!(
            damage_after < damage_before,
            "armor {after} must reduce damage ({damage_before} -> {damage_after})"
        );
        // Damage research reaches the same unit too.
        let damage_stat_before = sim.unit(footman).unwrap().damage;
        sim.research_upgrade(blacksmith, "Rhme")
            .expect("blacksmith researches melee attack");
        for _ in 0..(data::upgrade_def("Rhme").unwrap().research_ticks(1) + 30) {
            sim.step(&[]);
            if sim.upgrade_level(0, "Rhme") > 0 {
                break;
            }
        }
        assert_eq!(sim.upgrade_level(0, "Rhme"), 1);
        assert!(
            sim.unit(footman).unwrap().damage > damage_stat_before,
            "Rhme must raise the footman's damage"
        );
    }
}

mod models {
    use super::*;

    /// `content::models::all_present`: every building def points at a model the
    /// TFT archive really ships; roster 50/50 and roster+shops 54/54.
    #[test]
    fn all_present() {
        let mut roster = 0;
        let mut all = 0;
        for race in RACES {
            for def in data::building_defs(race) {
                assert!(
                    !def.model.is_empty(),
                    "{race:?} building {} has no model path",
                    def.id
                );
                assert!(
                    def.model_present,
                    "{race:?} building {} model {} is not in the archive",
                    def.id, def.model
                );
                all += 1;
            }
            roster += data::ROSTER
                .iter()
                .filter(|def| def.race == race && def.is_building())
                .count();
        }
        assert_eq!(roster, 50, "four-race roster buildings");
        assert_eq!(all, 54, "roster + melee shops");
    }
}

mod units {
    use super::*;

    /// `content.units.all-trainable-present` (`units::all_trainable_present`):
    /// every unit any race building trains (`Trains=` in `*UnitFunc.txt`,
    /// `Sellunits=` for the tavern) has a roster def, and the def names that
    /// building as its producer.  The
    /// census in `analysis/agent-sim/unit-gaps-*.txt` is this test's printout.
    #[test]
    fn all_trainable_present() {
        let mut total = 0usize;
        for race in RACES {
            let mut trained: Vec<&str> = Vec::new();
            for def in data::ROSTER.iter().filter(|def| def.race == race && def.is_building()) {
                for unit_id in def.trains {
                    total += 1;
                    assert!(
                        data::def_of(unit_id).is_some(),
                        "{race:?}: {} trains {unit_id}, which is not a def",
                        def.id
                    );
                    if !trained.contains(&unit_id) {
                        trained.push(unit_id);
                    }
                }
            }
            println!(
                "RACE {:<8} trains_total={} trains_present={} unit_ids=[{}]",
                format!("{race:?}"),
                trained.len(),
                trained.len(),
                trained.join(" ")
            );
            // The producers that the census found missing before ledger 472.
            for expected in match race {
                Race::Human => vec!["hfoo", "hrif", "hkni", "hspt", "hdhw", "hrtt", "hmtm", "hmtt", "hgyr", "hmpr", "hsor"],
                Race::Orc => vec!["ogru", "ohun", "otbk", "ocat", "otbr", "orai", "okod", "owyv", "oshm", "odoc", "ospm", "otau"],
                Race::Undead => vec!["ugho", "ucry", "ugar", "unec", "uban", "umtw", "uabo", "uobs", "ufro"],
                _ => vec!["edry", "edoc", "emtg", "earc", "esen", "ebal", "ehip", "edot", "efdr", "echm"],
            } {
                assert!(
                    trained.contains(&expected),
                    "{race:?}: {expected} is trainable in the install data but absent from the train lists"
                );
                let def = data::def_of(expected).unwrap_or_else(|| panic!("{expected}: no def"));
                assert!(
                    !def.trained_at.is_empty(),
                    "{expected} has no producer despite being trained"
                );
            }
        }
        assert!(total >= 60, "train links found: {total}");
        // The tavern sells its neutral hero roster through `Sellunits=`.
        let tavern = data::tavern().expect("tavern");
        assert_eq!(tavern.trains.len(), 5, "tavern hero card");
        for hero in tavern.trains {
            let def = data::def_of(hero).unwrap_or_else(|| panic!("tavern sells unknown {hero}"));
            assert!(def.hero, "{hero} sold by the tavern is not a hero");
        }
    }

    /// `content.units.models-present` (`units::models_present`): every
    /// trainable unit carries the real TFT/RoC model path and that model
    /// exists in the local archives.
    #[test]
    fn models_present() {
        let mut trainable = 0usize;
        let mut missing = Vec::new();
        for def in data::ROSTER.iter().filter(|def| !def.is_building()) {
            if def.trained_at.is_empty() {
                continue;
            }
            trainable += 1;
            assert!(!def.model.is_empty(), "{} has no model path", def.id);
            assert!(
                !def.icon.is_empty(),
                "{} has no command-button icon (UnitFunc Art=)",
                def.id
            );
            if !def.model_present {
                missing.push(format!("{}: {}", def.id, def.model));
            }
        }
        assert!(
            missing.is_empty(),
            "trainable units whose model is absent from the archives: {missing:?}"
        );
        assert_eq!(trainable, 71, "trainable unit defs");
    }

    /// `content.units.abilities-mapped` (`units::abilities_mapped`): every
    /// ability id a roster unit lists in `UnitAbilities.slk` resolves to a
    /// simulated spell or to an explicit data-only ability row -- never to
    /// nothing.
    #[test]
    fn abilities_mapped() {
        let mut refs = 0usize;
        let mut unresolved = Vec::new();
        let mut data_only = 0usize;
        for def in data::ROSTER.iter() {
            for ability_id in def.ability_ids {
                refs += 1;
                match wc3_sim::abilities::spell_for_ability(ability_id) {
                    Some(wc3_sim::abilities::Spell::DataOnly { id }) => {
                        assert_eq!(id, *ability_id);
                        data_only += 1;
                    }
                    Some(_) => {}
                    None => unresolved.push(format!("{}: {ability_id}", def.id)),
                }
            }
        }
        assert!(
            unresolved.is_empty(),
            "ability ids with no AbilityData row: {unresolved:?}"
        );
        assert!(refs >= 300, "ability refs: {refs}");
        println!("abilities refs={refs} data_only={data_only}");
        // The Spell Breaker is the census' worked example: its Control Magic
        // (`Acmg`) is granted by `Rhss`, and both resolve.
        let spell_breaker = data::def_of("hspt").expect("Spell Breaker in the roster");
        assert!(spell_breaker.ability_ids.contains(&"Acmg"));
        assert!(spell_breaker.trained_at.contains(&"hars"));
        assert!(
            wc3_sim::abilities::spell_for_ability("Acmg").is_some(),
            "Acmg must resolve"
        );
        assert_eq!(data::ability_unlock_upgrade("Acmg"), Some("Rhss"));
        // Every unit ability id is also reachable through the upgrade that
        // grants it, when the install data maps one.
        for def in data::ROSTER.iter().filter(|def| !def.is_building()) {
            for ability_id in def.ability_ids {
                if let Some(upgrade) = data::ability_unlock_upgrade(ability_id) {
                    assert!(
                        data::upgrade_def(upgrade).is_some(),
                        "{}: {ability_id} gated by unknown upgrade {upgrade}",
                        def.id
                    );
                }
            }
        }
    }
}

mod lumber_mill {
    use super::*;

    /// `content::lumber_mill::research_menu`: the Human Lumber Mill, Orc War
    /// Mill and Night Elf Hunter's Hall research menus are complete and every
    /// research reaches a unit (stat delta or ability grant).
    #[test]
    fn research_menu() {
        let cases: [(&str, &[&str]); 3] = [
            ("hlum", &["Rhac", "Rhlh"]),
            ("ofor", &["Rome", "Roar", "Rora", "Rosp", "Rorb"]),
            (
                "edob",
                &["Resm", "Rema", "Resw", "Rerh", "Reuv", "Rews"],
            ),
        ];
        for (building, expected) in cases {
            let def = data::def_of(building).expect(building);
            assert_same_set(
                def.researches,
                expected,
                &format!("{building} research menu"),
            );
            for research in def.researches {
                let upgrade = data::upgrade_def(research)
                    .unwrap_or_else(|| panic!("{building}: {research} missing"));
                assert!(
                    !upgrade.effects.is_empty(),
                    "{building}: {research} has no effect"
                );
                let reaches_unit = data::ROSTER
                    .iter()
                    .any(|unit| data::affected_by(unit, research) && unit.race == upgrade.race)
                    || upgrade.effects.iter().any(|effect| {
                        matches!(
                            effect.kind,
                            data::UpgradeEffectKind::UnlockAbility
                                | data::UpgradeEffectKind::DataOnly
                        )
                    });
                assert!(
                    reaches_unit,
                    "{building}: {research} ({}) reaches no unit",
                    upgrade.name
                );
            }
        }
        // Lumber Harvesting is a real carry-capacity effect on the peasant.
        let rlhl = data::upgrade_def("Rhlh").expect("Rhlh");
        assert!(rlhl.effects.iter().any(|effect| {
            effect.kind == data::UpgradeEffectKind::LumberHarvest && effect.base == 5
        }));
        assert!(data::affected_by(data::def_of("hpea").unwrap(), "Rhlh"));
        // Architecture (`Rhac`) is a building upgrade: town halls list it.
        assert!(data::affected_by(data::def_of("htow").unwrap(), "Rhac"));
    }
}
