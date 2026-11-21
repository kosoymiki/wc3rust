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

// <content-census:begin>
// @generated by analysis/agent-sim/gen_content_census.py -- do not edit.

/// Every `UnitBalance.slk` type id in the shipped extract.
pub const SLK_UNITS: &[&str] = &[
    "Ecen", "Edem", "Edmm", "Eevi", "Eevm", "Efur", "Eidm", "Eill",
    "Eilm", "Ekee", "Ekgg", "Emfr", "Emns", "Emoo", "Etyr", "Ewar",
    "Ewrd", "Hamg", "Hant", "Hapm", "Harf", "Hart", "Hblm", "Hdgo",
    "Hgam", "Hhkl", "Hjai", "Hkal", "Hlgr", "Hmbr", "Hmgd", "Hmkg",
    "Hpal", "Hpb1", "Hpb2", "Huth", "Hvsh", "Hvwd", "Naka", "Nbbc",
    "Nbrn", "Nbst", "Nkjx", "Nklj", "Nmag", "Nman", "Nngs", "Npbm",
    "Npld", "Nplh", "Nsjs", "Obla", "Ocb2", "Ocbh", "Odrt", "Ofar",
    "Ogld", "Ogrh", "Opgh", "Orex", "Orkn", "Osam", "Oshd", "Otcc",
    "Otch", "Othr", "Uanb", "Ubal", "Uclc", "Ucrl", "Udea", "Udre",
    "Udth", "Uear", "Uktl", "Ulic", "Umal", "Usyl", "Utic", "Uvar",
    "Uvng", "Uwar", "eaoe", "eaom", "eaow", "earc", "eate", "ebal",
    "ebsh", "echm", "edcm", "eden", "edes", "edob", "edoc", "edos",
    "edot", "edry", "edtm", "efdr", "efon", "egol", "ehip", "ehpr",
    "eilw", "emow", "emtg", "enec", "ensh", "esen", "eshd", "eshy",
    "espv", "etoa", "etoe", "etol", "etrp", "etrs", "even", "ewsp",
    "halt", "harm", "haro", "hars", "hatw", "hbar", "hbew", "hbla",
    "hbot", "hbsh", "hcas", "hcth", "hctw", "hdes", "hdhw", "hfoo",
    "hgra", "hgry", "hgtw", "hgyr", "hhdl", "hhes", "hhou", "hkee",
    "hkni", "hlum", "hmil", "hmpr", "hmtm", "hmtt", "hpea", "hphx",
    "hprt", "hpxe", "hrdh", "hrif", "hrtt", "hshy", "hsor", "hspt",
    "htow", "hvlt", "hwat", "hwt2", "hwt3", "hwtw", "nadk", "nadr",
    "nadw", "nahy", "nalb", "nanb", "nanc", "nane", "nanm", "nano",
    "nanw", "narg", "nass", "nbal", "nban", "nbda", "nbdk", "nbdm",
    "nbdo", "nbdr", "nbds", "nbdw", "nbee", "nbel", "nbfl", "nbld",
    "nbnb", "nbot", "nbrg", "nbse", "nbsm", "nbsp", "nbsw", "nbt1",
    "nbt2", "nbwd", "nbwm", "nbzd", "nbzk", "nbzw", "ncap", "ncat",
    "ncaw", "ncb0", "ncb1", "ncb2", "ncb3", "ncb4", "ncb5", "ncb6",
    "ncb7", "ncb8", "ncb9", "ncba", "ncbb", "ncbc", "ncbd", "ncbe",
    "ncbf", "ncea", "ncen", "ncer", "ncfs", "nchg", "nchp", "nchr",
    "nchw", "ncim", "nckb", "ncks", "ncmw", "ncnk", "ncnt", "ncop",
    "ncp2", "ncp3", "ncpn", "ncrb", "nct1", "nct2", "ncta", "ncte",
    "nctl", "ndch", "nder", "ndfl", "ndgt", "ndh0", "ndh1", "ndh2",
    "ndh3", "ndh4", "ndke", "ndkw", "ndmg", "ndmu", "ndog", "ndqn",
    "ndqp", "ndqs", "ndqt", "ndqv", "ndr1", "ndr2", "ndr3", "ndrb",
    "ndrd", "ndrf", "ndrg", "ndrh", "ndrj", "ndrk", "ndrl", "ndrm",
    "ndrn", "ndro", "ndrp", "ndrr", "ndrs", "ndrt", "ndru", "ndrv",
    "ndrw", "ndrz", "ndsa", "ndt1", "ndt2", "ndtb", "ndth", "ndtp",
    "ndtr", "ndtt", "ndtw", "ndwm", "nef0", "nef1", "nef2", "nef3",
    "nef4", "nef5", "nef6", "nef7", "nefm", "negf", "negm", "negt",
    "negz", "nehy", "nelb", "nele", "nemi", "nenc", "nenf", "nenp",
    "nepl", "nerd", "ners", "nerw", "net1", "net2", "nfbr", "nfel",
    "nfgb", "nfgl", "nfgo", "nfgt", "nfgu", "nfh0", "nfh1", "nfnp",
    "nfod", "nfoh", "nfor", "nfot", "nfov", "nfpc", "nfpe", "nfpl",
    "nfps", "nfpt", "nfpu", "nfr1", "nfr2", "nfra", "nfrb", "nfre",
    "nfrg", "nfrl", "nfrm", "nfrp", "nfrs", "nfrt", "nfsh", "nfsp",
    "nft1", "nft2", "nftb", "nftk", "nftr", "nftt", "nfv0", "nfv1",
    "nfv2", "nfv3", "nfv4", "ngad", "ngbl", "ngdk", "nggr", "ngh1",
    "ngh2", "ngir", "nglm", "ngme", "ngna", "ngnb", "ngnh", "ngni",
    "ngno", "ngns", "ngnv", "ngnw", "ngob", "ngol", "ngrd", "ngrk",
    "ngrw", "ngsp", "ngst", "ngt2", "ngwr", "ngz1", "ngz2", "ngz3",
    "ngz4", "ngza", "ngzc", "ngzd", "nhar", "nhcn", "nhdc", "nhea",
    "nheb", "nhef", "nhem", "nhew", "nhfp", "nhhr", "nhmc", "nhns",
    "nhrh", "nhrq", "nhrr", "nhrw", "nhyc", "nhyd", "nhyh", "nhym",
    "nico", "nina", "ninc", "ninf", "ninm", "nitb", "nith", "nitp",
    "nitr", "nits", "nitt", "nitw", "njg1", "njga", "njgb", "njks",
    "nkob", "nkog", "nkol", "nkot", "nlds", "nlkl", "nlpd", "nlpr",
    "nlps", "nlrv", "nlsn", "nltc", "nltl", "nlur", "nmam", "nmbg",
    "nmcf", "nmdm", "nmdr", "nmed", "nmer", "nmfs", "nmg0", "nmg1",
    "nmgd", "nmgr", "nmgv", "nmgw", "nmh0", "nmh1", "nmit", "nmmu",
    "nmoo", "nmpe", "nmpg", "nmr0", "nmr2", "nmr3", "nmr4", "nmr5",
    "nmr6", "nmr7", "nmr8", "nmr9", "nmra", "nmrb", "nmrc", "nmrd",
    "nmre", "nmrf", "nmrk", "nmrl", "nmrm", "nmrr", "nmrv", "nmsc",
    "nmsh", "nmsn", "nmtw", "nmyr", "nmys", "nnad", "nndk", "nndr",
    "nnfm", "nnht", "nnmg", "nnrg", "nnrs", "nnsa", "nnsg", "nnsu",
    "nnsw", "nntg", "nntt", "nnwa", "nnwl", "nnwq", "nnwr", "nnws",
    "nnzg", "noga", "nogl", "nogm", "nogn", "nogo", "nogr", "nomg",
    "now2", "now3", "nowb", "nowe", "nowk", "nowl", "npfl", "npfm",
    "npgf", "npgr", "npig", "nplb", "nplg", "npn1", "npn2", "npn3",
    "npn4", "npn5", "npn6", "npng", "npnw", "nqb1", "nqb2", "nqb3",
    "nqb4", "nqbh", "nrat", "nrdk", "nrdr", "nrel", "nrog", "nrvd",
    "nrvf", "nrvi", "nrvl", "nrvs", "nrwm", "nrzb", "nrzg", "nrzm",
    "nrzs", "nrzt", "nsat", "nsbm", "nsbs", "nsc2", "nsc3", "nscb",
    "nsea", "nsel", "nser", "nsgb", "nsgg", "nsgh", "nsgn", "nsgt",
    "nsha", "nshe", "nshf", "nshp", "nshr", "nshw", "nska", "nske",
    "nskf", "nskg", "nskk", "nskm", "nsko", "nslf", "nslh", "nsll",
    "nslm", "nsln", "nslr", "nslv", "nsno", "nsnp", "nsns", "nsoc",
    "nsog", "nspb", "nspc", "nspd", "nspg", "nspp", "nspr", "nsqa",
    "nsqe", "nsqo", "nsqt", "nsra", "nsrh", "nsrn", "nsrv", "nsrw",
    "nssn", "nssp", "nsth", "nstl", "nsts", "nstw", "nsty", "nsw1",
    "nsw2", "nsw3", "ntav", "nten", "nth0", "nth1", "nthl", "nthr",
    "ntka", "ntkc", "ntkf", "ntkh", "ntks", "ntkt", "ntkw", "ntn2",
    "ntnt", "ntor", "ntrd", "ntrg", "ntrh", "ntrs", "ntrt", "ntrv",
    "ntt1", "ntt2", "ntws", "ntx2", "nubk", "nubr", "nubw", "nvde",
    "nvdg", "nvdl", "nvdw", "nvil", "nvk2", "nvl2", "nvlk", "nvlw",
    "nvr0", "nvr1", "nvr2", "nvul", "nw2w", "nwad", "nwat", "nwc1",
    "nwc2", "nwe1", "nwe2", "nwe3", "nwen", "nwgs", "nwgt", "nwiz",
    "nwld", "nwlg", "nwlt", "nwna", "nwnr", "nwns", "nwrg", "nws1",
    "nwwd", "nwwf", "nwwg", "nwzd", "nwzg", "nwzr", "nzep", "nzin",
    "nzlc", "nzom", "oalt", "oang", "obar", "obea", "obot", "ocat",
    "ocbw", "odes", "odkt", "odoc", "oeye", "ofor", "ofrt", "ogre",
    "ogrk", "ogru", "ohun", "ohwd", "ojgn", "okod", "omtg", "onzg",
    "oosc", "opeo", "orai", "oshm", "oshy", "osld", "osp1", "osp2",
    "osp3", "osp4", "ospm", "ospw", "ostr", "osw1", "osw2", "osw3",
    "oswy", "otau", "otbk", "otbr", "otot", "otrb", "otto", "ovlj",
    "ovln", "owar", "ownr", "owtw", "owyv", "uabc", "uabo", "uaco",
    "uaod", "uarb", "uban", "ubdd", "ubdr", "ubon", "ubot", "ubsp",
    "ucrm", "ucry", "ucs1", "ucs2", "ucs3", "ucsB", "ucsC", "udes",
    "ufro", "ugar", "ugho", "ugol", "ugrm", "ugrv", "uktg", "uktn",
    "uloc", "umtw", "unec", "unp1", "unp2", "unpl", "uobs", "uplg",
    "usap", "usep", "ushd", "ushp", "uske", "uskm", "uslh", "uswb",
    "utod", "utom", "uubs", "uzg1", "uzg2", "uzig", "zcso", "zhyd",
    "zjug", "zmar", "zshv", "zsmc", "zzrg",
];

/// Unit ids reachable from the four melee tech trees
/// (`Builds=`/`Trains=`/`Sellunits=`/`Upgrade=` closure of the profiles).
pub const MELEE_UNITS: &[&str] = &[
    "Edem", "Ekee", "Emoo", "Ewar", "Hamg", "Hblm", "Hmkg", "Hpal",
    "Obla", "Ofar", "Oshd", "Otch", "Ucrl", "Udea", "Udre", "Ulic",
    "eaoe", "eaom", "eaow", "earc", "eate", "ebal", "echm", "eden",
    "edob", "edoc", "edos", "edot", "edry", "efdr", "ehip", "emow",
    "emtg", "esen", "etoa", "etoe", "etol", "etrp", "ewsp", "halt",
    "harm", "hars", "hatw", "hbar", "hbla", "hcas", "hctw", "hdhw",
    "hfoo", "hgra", "hgry", "hgtw", "hgyr", "hhou", "hkee", "hkni",
    "hlum", "hmpr", "hmtm", "hmtt", "hpea", "hrif", "hrtt", "hsor",
    "hspt", "htow", "hvlt", "hwtw", "oalt", "obar", "obea", "ocat",
    "odoc", "ofor", "ofrt", "ogre", "ogru", "ohun", "okod", "opeo",
    "orai", "oshm", "osld", "ospm", "ostr", "otau", "otbk", "otbr",
    "otrb", "otto", "ovln", "owtw", "owyv", "uabo", "uaco", "uaod",
    "uban", "ubon", "ucry", "ufro", "ugar", "ugho", "ugol", "ugrv",
    "umtw", "unec", "unp1", "unp2", "unpl", "uobs", "usap", "usep",
    "uslh", "utod", "utom", "uzg1", "uzg2", "uzig",
];

/// `isbldg=1` rows inside the melee closure.
pub const MELEE_BUILDINGS: &[&str] = &[
    "eaoe", "eaom", "eaow", "eate", "eden", "edob", "edos", "emow",
    "etoa", "etoe", "etol", "etrp", "halt", "harm", "hars", "hatw",
    "hbar", "hbla", "hcas", "hctw", "hgra", "hgtw", "hhou", "hkee",
    "hlum", "htow", "hvlt", "hwtw", "oalt", "obar", "obea", "ofor",
    "ofrt", "ogre", "osld", "ostr", "otrb", "otto", "ovln", "owtw",
    "uaod", "ubon", "ugol", "ugrv", "unp1", "unp2", "unpl", "usap",
    "usep", "uslh", "utod", "utom", "uzg1", "uzg2", "uzig",
];

/// Unit ids placed by the shipped campaign maps that also
/// have a `UnitBalance.slk` row.
pub const CAMPAIGN_UNITS: &[&str] = &[
    "Ecen", "Efur", "Eill", "Ekee", "Etyr", "Hamg", "Hant", "Harf",
    "Hart", "Hjai", "Hmbr", "Hmkg", "Hpal", "Hpb1", "Hpb2", "Huth",
    "Hvwd", "Nman", "Npld", "Obla", "Ocbh", "Ogrh", "Opgh", "Othr",
    "Udre", "Uear", "Uktl", "Ulic", "Umal", "Utic", "Uwar", "eaoe",
    "eaom", "eaow", "earc", "eate", "ebal", "echm", "edcm", "edob",
    "edoc", "edos", "edry", "ehpr", "emow", "esen", "eshd", "etoa",
    "etoe", "etol", "etrp", "ewsp", "halt", "harm", "hars", "hbar",
    "hbla", "hcas", "hcth", "hctw", "hfoo", "hgra", "hgry", "hgtw",
    "hgyr", "hhdl", "hhes", "hhou", "hkee", "hkni", "hlum", "hmpr",
    "hmtm", "hmtt", "hpea", "hrif", "hsor", "htow", "hwat", "hwt2",
    "hwtw", "nadk", "nadw", "nass", "nbal", "nban", "nbdk", "nbdr",
    "nbfl", "nbrg", "nbsm", "nbsp", "nbwd", "nbzw", "ncap", "ncaw",
    "ncb0", "ncb1", "ncb2", "ncb3", "ncb4", "ncb5", "ncb6", "ncb7",
    "ncb8", "ncb9", "ncba", "ncbb", "ncbc", "ncbd", "ncbe", "ncbf",
    "ncea", "ncen", "ncer", "nchg", "nchr", "nchw", "ncim", "nckb",
    "ncks", "ncmw", "ncnk", "ncnt", "ncop", "ncp3", "ncpn", "nct1",
    "nct2", "ncte", "nctl", "nder", "ndfl", "ndgt", "ndmg", "ndog",
    "ndrb", "ndtb", "ndth", "ndtp", "ndtr", "ndtt", "ndtw", "nef0",
    "nef1", "nef2", "nef3", "nef4", "nef6", "nef7", "nefm", "negf",
    "negm", "negt", "nenc", "nenf", "nenp", "nepl", "nfel", "nfh0",
    "nfh1", "nfoh", "nfr1", "nfr2", "nfra", "nfrb", "nfre", "nfrg",
    "nfrl", "nfrm", "nfrp", "nfrs", "nfsh", "nftb", "nftr", "nftt",
    "ngad", "nggr", "ngh1", "ngh2", "ngir", "nglm", "ngme", "ngna",
    "ngnb", "ngnh", "ngni", "ngno", "ngns", "ngnv", "ngnw", "ngob",
    "ngrk", "ngsp", "ngst", "ngt2", "ngwr", "nhar", "nhcn", "nhea",
    "nheb", "nhef", "nhem", "nhfp", "nhhr", "nhns", "nhrh", "nhrq",
    "nhrr", "nhrw", "ninf", "nith", "nitp", "nitr", "nits", "nitt",
    "nitw", "nkob", "nkog", "nkol", "nkot", "nltl", "nmdm", "nmed",
    "nmer", "nmfs", "nmgv", "nmh0", "nmh1", "nmmu", "nmoo", "nmpg",
    "nmr7", "nmrl", "nmrm", "nmrr", "nnwa", "nnwl", "nnwq", "nnwr",
    "nnws", "nnzg", "nogl", "nogm", "nogr", "nomg", "nowb", "nowe",
    "nowk", "npgf", "npig", "nqbh", "nrat", "nrdk", "nrog", "nrvd",
    "nrvf", "nrvi", "nrvs", "nrwm", "nrzb", "nrzg", "nrzm", "nrzs",
    "nrzt", "nsat", "nsbm", "nsea", "nser", "nsgg", "nsgt", "nshe",
    "nshr", "nska", "nskf", "nskg", "nskm", "nslf", "nsll", "nslm",
    "nsln", "nslr", "nslv", "nspd", "nspg", "nspr", "nsqe", "nsqt",
    "nssn", "nssp", "nsth", "nstl", "nsts", "nstw", "nsty", "nten",
    "nth0", "nth1", "nthl", "nthr", "ntn2", "ntnt", "ntt2", "nvil",
    "nvk2", "nvl2", "nvlk", "nvlw", "nvul", "nw2w", "nwat", "nwc1",
    "nwc2", "nwen", "nwgt", "nwiz", "nwld", "nwlg", "nwlt", "nwna",
    "nwnr", "nwns", "nwrg", "nws1", "nwwd", "nwwf", "nwwg", "nwzr",
    "nzep", "oalt", "obar", "obea", "ocat", "ocbw", "odoc", "ofor",
    "ofrt", "ogre", "ogru", "ohun", "okod", "oosc", "opeo", "orai",
    "oshm", "osld", "ostr", "otau", "otrb", "otto", "ownr", "owtw",
    "owyv", "uabo", "uaco", "uaod", "uban", "ubon", "ucry", "ufro",
    "ugar", "ugho", "ugrm", "ugrv", "uktg", "uktn", "umtw", "unec",
    "unp1", "unp2", "unpl", "usap", "usep", "ushd", "uske", "uslh",
    "utod", "uzg1", "uzig",
];

/// Every `UpgradeData.slk` id.
pub const SLK_UPGRADES: &[&str] = &[
    "Recb", "Redc", "Redt", "Reeb", "Reec", "Rehs", "Reht", "Reib",
    "Rema", "Remg", "Remk", "Renb", "Repb", "Repm", "Rerh", "Rers",
    "Resc", "Resi", "Resm", "Resw", "Reuv", "Rews", "Rgfo", "Rguv",
    "Rhac", "Rhan", "Rhar", "Rhcd", "Rhde", "Rhfc", "Rhfl", "Rhfs",
    "Rhgb", "Rhhb", "Rhla", "Rhlh", "Rhme", "Rhpm", "Rhpt", "Rhra",
    "Rhri", "Rhrt", "Rhse", "Rhss", "Rhst", "Rnam", "Rnat", "Rnen",
    "Rnsb", "Rnsi", "Rnsw", "Roar", "Robf", "Robk", "Robs", "Roch",
    "Roen", "Rolf", "Rome", "Ropg", "Ropm", "Rora", "Rorb", "Rosp",
    "Rost", "Rotr", "Rovs", "Rowd", "Rows", "Rowt", "Ruac", "Ruar",
    "Ruba", "Rubu", "Rucr", "Ruex", "Rufb", "Rugf", "Rume", "Rune",
    "Rupc", "Rupm", "Rura", "Rusf", "Rusl", "Rusm", "Rusp", "Ruwb",
    "Rwdm",
];

/// Every `ItemData.slk` id.
pub const SLK_ITEMS: &[&str] = &[
    "afac", "ajen", "amrc", "anfg", "ankh", "arsc", "arsh", "asbl",
    "axas", "azhr", "belv", "bfhr", "bgst", "blba", "brac", "brag",
    "bspd", "btst", "bzbe", "bzbf", "ccmd", "ches", "ciri", "ckng",
    "clfm", "clsd", "cnhn", "cnob", "cosl", "crdt", "crys", "desc",
    "dkfw", "dphe", "drph", "dsum", "dthb", "dtsb", "dust", "engs",
    "envl", "esaz", "evtl", "fgdg", "fgfh", "fgrd", "fgrg", "fgsk",
    "fgun", "flag", "frgd", "frhg", "fwss", "gcel", "gemt", "gfor",
    "gldo", "glsk", "gmfr", "gobm", "gold", "gomn", "gopr", "grsl",
    "gsou", "guvi", "gvsm", "hbth", "hcun", "hlst", "horl", "hslv",
    "hval", "infs", "iwbr", "jdrn", "jpnt", "k3m1", "k3m2", "k3m3",
    "kgal", "klmm", "kpin", "ktrm", "kybl", "kygh", "kymn", "kysn",
    "ledg", "lgdh", "lhst", "lmbr", "lnrn", "lure", "manh", "mcou",
    "mcri", "mgtk", "mlst", "mnsf", "mnst", "modt", "moon", "mort",
    "nflg", "nspi", "ocor", "odef", "ofir", "oflg", "ofro", "oli2",
    "olig", "oslo", "oven", "pams", "pclr", "pdiv", "penr", "pghe",
    "pgin", "pgma", "phea", "phlt", "pinv", "plcl", "pman", "pmna",
    "pnvl", "pnvu", "pomn", "pres", "prvt", "pspd", "rag1", "ram1",
    "ram2", "ram3", "ram4", "rat3", "rat6", "rat9", "ratc", "ratf",
    "rde0", "rde1", "rde2", "rde3", "rde4", "rdis", "rej1", "rej2",
    "rej3", "rej4", "rej5", "rej6", "rhe1", "rhe2", "rhe3", "rhth",
    "rin1", "rlif", "rma2", "rman", "rnec", "rnsp", "rots", "rre1",
    "rre2", "rreb", "rres", "rspd", "rspl", "rsps", "rst1", "rugt",
    "rump", "rwat", "rwiz", "sand", "sbch", "sbok", "schl", "sclp",
    "scul", "sehr", "sfog", "shar", "shas", "shcw", "shdt", "shea",
    "shen", "shhn", "shrs", "shtm", "shwd", "silk", "skrt", "sksh",
    "skul", "sman", "sneg", "sor1", "sor2", "sor3", "sor4", "sor5",
    "sor6", "sor7", "sor8", "sor9", "sora", "sorf", "soul", "spre",
    "sprn", "spro", "spsh", "srbd", "sreg", "sres", "sror", "srrc",
    "srtl", "ssan", "ssil", "stel", "stpg", "stre", "stwa", "stwp",
    "tbak", "tbar", "tbsm", "tcas", "tdex", "tdx2", "tels", "texp",
    "tfar", "tgrh", "tgxp", "thdm", "thle", "tin2", "tint", "tkno",
    "tlum", "tmmt", "tmsc", "totw", "tpow", "tret", "tsct", "tst2",
    "tstr", "uflg", "vamp", "vddl", "ward", "wcyc", "whwd", "wild",
    "will", "wlsd", "wneg", "wneu", "wolg", "woms", "wshs", "wswd",
    "wtlg",
];

/// Every `AbilityData.slk` id in the TFT extract (the shipped ruleset).
pub const SLK_ABILITIES: &[&str] = &[
    "ACac", "ACad", "ACah", "ACam", "ACat", "ACav", "ACba", "ACbb",
    "ACbc", "ACbf", "ACbh", "ACbk", "ACbl", "ACbn", "ACbz", "ACc2",
    "ACc3", "ACca", "ACcb", "ACce", "ACch", "ACcl", "ACcn", "ACcr",
    "ACcs", "ACct", "ACcv", "ACcw", "ACcy", "ACd2", "ACdc", "ACde",
    "ACdm", "ACdr", "ACds", "ACdv", "ACen", "ACes", "ACev", "ACf2",
    "ACf3", "ACfa", "ACfb", "ACfd", "ACff", "ACfl", "ACfn", "ACfr",
    "ACfs", "ACfu", "AChv", "AChw", "AChx", "ACif", "ACim", "ACls",
    "ACm2", "ACm3", "ACmf", "ACmi", "ACmo", "ACmp", "ACnr", "ACpa",
    "ACps", "ACpu", "ACpv", "ACpy", "ACr1", "ACr2", "ACrd", "ACrf",
    "ACrg", "ACrj", "ACrk", "ACrn", "ACro", "ACs7", "ACs8", "ACs9",
    "ACsa", "ACsf", "ACsh", "ACsi", "ACsk", "ACsl", "ACsm", "ACsp",
    "ACss", "ACst", "ACsw", "ACt2", "ACtb", "ACtc", "ACtn", "ACua",
    "ACuf", "ACvp", "ACvs", "ACwb", "ACwe", "AEIl", "AEah", "AEar",
    "AEbl", "AEbu", "AEer", "AEev", "AEfk", "AEfn", "AEim", "AEmb",
    "AEme", "AEpa", "AEsb", "AEsf", "AEsh", "AEst", "AEsv", "AEtq",
    "AEvi", "AGbu", "AHab", "AHad", "AHav", "AHbh", "AHbn", "AHbu",
    "AHbz", "AHca", "AHdr", "AHds", "AHer", "AHfa", "AHfs", "AHhb",
    "AHmt", "AHpx", "AHre", "AHta", "AHtb", "AHtc", "AHwe", "AI2m",
    "AIa1", "AIa3", "AIa4", "AIa6", "AIaa", "AIad", "AIae", "AIam",
    "AIan", "AIar", "AIat", "AIau", "AIav", "AIaz", "AIba", "AIbb",
    "AIbf", "AIbg", "AIbh", "AIbk", "AIbl", "AIbm", "AIbr", "AIbs",
    "AIbt", "AIbx", "AIcb", "AIcd", "AIcf", "AIcl", "AIcm", "AIco",
    "AIcs", "AIct", "AIcy", "AId0", "AId1", "AId2", "AId3", "AId4",
    "AId5", "AId7", "AId8", "AIda", "AIdb", "AIdc", "AIdd", "AIdf",
    "AIdi", "AIdm", "AIdn", "AIdp", "AIds", "AIdv", "AIe2", "AIem",
    "AIev", "AIfa", "AIfb", "AIfd", "AIfe", "AIff", "AIfg", "AIfh",
    "AIfl", "AIfm", "AIfn", "AIfo", "AIfr", "AIfs", "AIft", "AIfu",
    "AIfw", "AIfx", "AIfz", "AIgd", "AIgf", "AIgm", "AIgo", "AIgu",
    "AIgx", "AIh1", "AIh2", "AIh3", "AIha", "AIhb", "AIhl", "AIhw",
    "AIhx", "AIi1", "AIi3", "AIi4", "AIi6", "AIil", "AIim", "AIin",
    "AIir", "AIl1", "AIl2", "AIlb", "AIlf", "AIll", "AIlm", "AIlp",
    "AIls", "AIlu", "AIlx", "AIlz", "AIm1", "AIm2", "AImb", "AImh",
    "AImo", "AImr", "AIms", "AImt", "AImv", "AImx", "AImz", "AInd",
    "AInm", "AInv", "AIob", "AIos", "AIp1", "AIp2", "AIp3", "AIp4",
    "AIp5", "AIp6", "AIpb", "AIpg", "AIpl", "AIpm", "AIpr", "AIps",
    "AIpv", "AIpx", "AIpz", "AIra", "AIrb", "AIrc", "AIrd", "AIre",
    "AIri", "AIrl", "AIrm", "AIrn", "AIrr", "AIrs", "AIrt", "AIrv",
    "AIrx", "AIs1", "AIs2", "AIs3", "AIs4", "AIs6", "AIsa", "AIsb",
    "AIse", "AIsh", "AIsi", "AIsl", "AIsm", "AIso", "AIsp", "AIsr",
    "AIsw", "AIsx", "AIsz", "AIt6", "AIt9", "AIta", "AItb", "AItc",
    "AItf", "AItg", "AIth", "AIti", "AItj", "AItk", "AItl", "AItm",
    "AItn", "AItp", "AItx", "AIuf", "AIuv", "AIuw", "AIv1", "AIv2",
    "AIva", "AIvl", "AIvu", "AIwb", "AIwm", "AIx1", "AIx2", "AIx3",
    "AIx4", "AIx5", "AIxk", "AIxm", "AIxs", "AIzb", "ANak", "ANav",
    "ANb2", "ANba", "ANbf", "ANbh", "ANbl", "ANbr", "ANbs", "ANbu",
    "ANca", "ANcf", "ANch", "ANcl", "ANdb", "ANdc", "ANdh", "ANdo",
    "ANdp", "ANdr", "ANef", "ANen", "ANfa", "ANfb", "ANfd", "ANfl",
    "ANfs", "ANha", "ANht", "ANhw", "ANhx", "ANin", "ANmo", "ANmr",
    "ANms", "ANpa", "ANpi", "ANpr", "ANr2", "ANr3", "ANrc", "ANre",
    "ANrf", "ANrn", "ANsa", "ANsb", "ANse", "ANsg", "ANsh", "ANsi",
    "ANsl", "ANsq", "ANss", "ANst", "ANsw", "ANt2", "ANta", "ANth",
    "ANto", "ANtr", "ANwk", "ANwm", "AOae", "AObu", "AOcl", "AOcr",
    "AOeq", "AOfs", "AOhw", "AOhx", "AOls", "AOmi", "AOr2", "AOr3",
    "AOre", "AOs2", "AOsf", "AOsh", "AOsw", "AOvd", "AOw2", "AOwk",
    "AOws", "AOww", "APdi", "APh1", "APh2", "APh3", "APmg", "APmr",
    "APra", "APrl", "APrr", "APsa", "APwt", "ARal", "AUan", "AUau",
    "AUav", "AUbu", "AUcb", "AUcs", "AUdc", "AUdd", "AUdp", "AUdr",
    "AUds", "AUfa", "AUfn", "AUfu", "AUim", "AUin", "AUls", "AUsl",
    "AUts", "Aabr", "Aabs", "Aadm", "Aaha", "Aakb", "Aall", "Aalr",
    "Aamk", "Aams", "Aap1", "Aap2", "Aap3", "Aap4", "Aasl", "Aast",
    "Aatk", "Aave", "Aawa", "Abdl", "Abds", "Abdt", "Abgl", "Abgm",
    "Abgs", "Ablo", "Ablp", "Abof", "Abrf", "Absk", "Abtl", "Abu2",
    "Abu3", "Abu5", "Abun", "Abur", "Acan", "Acdb", "Acdh", "Acef",
    "Achd", "Ache", "Achl", "Acht", "Aclf", "Acmg", "Acn2", "Acny",
    "Aco2", "Aco3", "Acoa", "Acoh", "Acor", "Acpf", "Acri", "Acrs",
    "Acyc", "Adch", "Adcn", "Adda", "Adec", "Adef", "Adev", "Adis",
    "Adri", "Adro", "Adsm", "Adt1", "Adtg", "Adtn", "Adts", "Advc",
    "Advm", "Aeat", "Aegm", "Aenc", "Aenr", "Aens", "Aent", "Aenw",
    "Aesn", "Aesr", "Aetf", "Aeth", "Aetl", "Aexh", "Aeye", "Afa2",
    "Afae", "Afak", "Afbb", "Afbk", "Afbt", "Afih", "Afin", "Afio",
    "Afir", "Afiu", "Afla", "Aflk", "Afod", "Afra", "Afrb", "Afrz",
    "Afsh", "Afzy", "Agho", "Agld", "Agra", "Agyb", "Agyd", "Agyv",
    "Ahar", "Ahea", "Ahid", "Ahnl", "Ahr2", "Ahr3", "Ahrl", "Ahrp",
    "Ahwd", "Aien", "Aihn", "Aimp", "Ainf", "Aion", "Aiun", "Aivs",
    "Alam", "Aliq", "Alit", "Aloa", "Aloc", "Alsh", "Amb2", "Ambb",
    "Ambd", "Ambt", "Amdf", "Amec", "Amed", "Amel", "Amfl", "Amgl",
    "Amgr", "Amic", "Amil", "Amim", "Amin", "Amls", "Amnb", "Amnx",
    "Amnz", "Amov", "Amrf", "Andm", "Andt", "Ane2", "Aneu", "Anh1",
    "Anh2", "Anhe", "Ansk", "Ansp", "Aoar", "Apak", "Aphx", "Apig",
    "Apit", "Apiv", "Aply", "Apmf", "Apo2", "Apoi", "Apos", "Aprg",
    "Apsh", "Apts", "Apxf", "Ara2", "Arai", "Arav", "Arbr", "Arej",
    "Arel", "Aren", "Arep", "Aret", "Arev", "Argd", "Argl", "Arll",
    "Arlm", "Arng", "Aro1", "Aro2", "Aroa", "Aroc", "Arpb", "Arpl",
    "Arpm", "Arsg", "Arsk", "Arsp", "Arsq", "Arst", "Arsw", "Asac",
    "Asal", "Asb1", "Asb2", "Asb3", "Asds", "Ashm", "Ashs", "Asid",
    "Asla", "Aslo", "Aslp", "Asod", "Asou", "Asp1", "Asp2", "Asp3",
    "Asp4", "Asp5", "Asp6", "Aspa", "Aspb", "Aspd", "Asph", "Aspi",
    "Aspl", "Aspo", "Aspp", "Asps", "Aspt", "Aspy", "Assk", "Assp",
    "Asta", "Astd", "Aste", "Asth", "Astn", "Asud", "Atau", "Atdg",
    "Atdp", "Atlp", "Atol", "Atru", "Atsp", "Attu", "Atwa", "Auco",
    "Auhf", "Ault", "Auns", "Aven", "Avng", "Avul", "Awan", "Awar",
    "Aweb", "Awfb", "Awh2", "Awha", "Awrg", "Awrh", "Awrp", "Awrs",
    "SCae", "SCc1", "SCva", "SNdc", "SNdd", "SNeq", "SNin", "Sbsk",
    "Sbtl", "Sca1", "Sca2", "Sca3", "Sca4", "Sca5", "Sca6", "Sch2",
    "Sch3", "Sch4", "Sch5", "Scri", "Sdro", "Slo2", "Slo3", "Sloa",
    "Srtt", "Sshm", "Suhf",
];

/// RoC-only `AbilityData.slk` rows (`useInEditor=0`, unreferenced).
pub const SLK_ABILITIES_ROC_ONLY: &[&str] = &[
    "Stpm", "Stpr",
];

/// Every `DestructableData.slk` id.
pub const SLK_DESTRUCTABLES: &[&str] = &[
    "ATg1", "ATg2", "ATg3", "ATg4", "ATt0", "ATt1", "ATtc", "ATtr",
    "ATwf", "BTrs", "BTrx", "BTsc", "BTtc", "BTtw", "CTtc", "CTtr",
    "DTc1", "DTc2", "DTep", "DTes", "DTfp", "DTfx", "DTg1", "DTg2",
    "DTg3", "DTg4", "DTg5", "DTg6", "DTg7", "DTg8", "DTlv", "DTrc",
    "DTrf", "DTrx", "DTs1", "DTs2", "DTs3", "DTsb", "DTsh", "DTsp",
    "Dofv", "Dofw", "FTtw", "GTsh", "IOt0", "IOt1", "IOt2", "ITag",
    "ITcr", "ITf1", "ITf2", "ITf3", "ITf4", "ITg1", "ITg2", "ITg3",
    "ITg4", "ITi2", "ITi3", "ITi4", "ITib", "ITig", "ITtc", "ITtg",
    "ITtw", "ITw0", "ITw1", "ITw2", "ITw3", "ITx1", "ITx2", "ITx3",
    "ITx4", "JTct", "JTtw", "KTtw", "LOcg", "LT00", "LT01", "LT02",
    "LT03", "LT04", "LT05", "LT06", "LT07", "LT08", "LT09", "LT10",
    "LT11", "LTba", "LTbr", "LTbs", "LTbx", "LTcr", "LTe1", "LTe2",
    "LTe3", "LTe4", "LTex", "LTg1", "LTg2", "LTg3", "LTg4", "LTlt",
    "LTr1", "LTr2", "LTr3", "LTr4", "LTr5", "LTr6", "LTr7", "LTr8",
    "LTrc", "LTs1", "LTs2", "LTs3", "LTs4", "LTs5", "LTs6", "LTs7",
    "LTs8", "LTt0", "LTt1", "LTt2", "LTt3", "LTt4", "LTt5", "LTtc",
    "LTtx", "LTw0", "LTw1", "LTw2", "LTw3", "NTbd", "NTtc", "NTtw",
    "OTds", "OTip", "OTis", "OTsp", "OTtw", "VTlt", "WTst", "WTtw",
    "XOk1", "XOk2", "XOkt", "XTbd", "XTm5", "XTmp", "XTmx", "XTvt",
    "XTx5", "YSdb", "YSdc", "YT00", "YT01", "YT02", "YT03", "YT04",
    "YT05", "YT06", "YT07", "YT08", "YT09", "YT10", "YT11", "YT12",
    "YT13", "YT14", "YT15", "YT16", "YT17", "YT18", "YT19", "YT20",
    "YT21", "YT22", "YT23", "YT24", "YT25", "YT26", "YT27", "YT28",
    "YT29", "YT30", "YT31", "YT32", "YT33", "YT34", "YT35", "YT36",
    "YT37", "YT38", "YT39", "YT40", "YT41", "YT42", "YT43", "YT44",
    "YT45", "YT46", "YT47", "YT48", "YT49", "YT50", "YT51", "YTab",
    "YTac", "YTce", "YTct", "YTcx", "YTfb", "YTfc", "YTft", "YTlb",
    "YTpb", "YTpc", "YTst", "YTwt", "Ytlc", "ZTd1", "ZTd2", "ZTd3",
    "ZTd4", "ZTd5", "ZTd6", "ZTd7", "ZTd8", "ZTg1", "ZTg2", "ZTg3",
    "ZTg4", "ZTnc", "ZTr0", "ZTr1", "ZTr2", "ZTr3", "ZTsg", "ZTsx",
    "ZTtc", "ZTtw", "ZTw0", "ZTw1", "ZTw2", "ZTw3",
];

/// Orc01: unit ids that have a shipped SLK row.
#[allow(dead_code)]
pub const ORC01_UNITS: &[&str] = &[
    "Ocbh", "Othr", "nbzw", "ncea", "ncer", "ncim", "ncks", "nct1",
    "nfoh", "nhar", "nhrw", "nltl", "nmfs", "nmh0", "nmh1", "nmrl",
    "nmrm", "nmrr", "npig", "nqbh", "nrzb", "nrzm", "nrzs", "nrzt",
    "nthl", "ntnt", "ntt2", "ogru", "ohun", "otau",
];

/// Orc01: ids defined only by the map's `war3map.w3u`.
#[allow(dead_code)]
pub const ORC01_MAP_ONLY_UNITS: &[&str] = &[
    "nC08", "nC09", "nC10", "oC03",
];

/// Orc01 map-object ids and the install-table unit they extend
/// (`war3map.w3u` custom object -> base id).
pub const ORC01_MAP_ONLY_BASE: &[(&str, &str)] = &[
    ("nC08", "ncnt"),
    ("nC09", "nct1"),
    ("nC10", "nct2"),
    ("oC03", "otau"),
];

/// Orc01: item ids.
#[allow(dead_code)]
pub const ORC01_ITEMS: &[&str] = &[
    "hlst", "shea", "whwd",
];

/// Orc01: destructable ids carried by the map.
#[allow(dead_code)]
pub const ORC01_DESTRUCTABLES: &[&str] = &[
    "BTtw", "LTcr",
];

/// Prologue01: unit ids that have a shipped SLK row.
#[allow(dead_code)]
pub const PROLOGUE01_UNITS: &[&str] = &[
    "Othr", "nban", "nbrg", "ncp3", "nder", "nfh0", "nfh1", "nftb",
    "nftr", "nftt", "ngna", "ngnh", "ngno", "ngt2", "nmdm", "nmh0",
    "nmh1", "nmrl", "nogr", "npgf", "npig", "obar", "ogru", "owtw",
];

/// Prologue01: ids defined only by the map's `war3map.w3u`.
#[allow(dead_code)]
pub const PROLOGUE01_MAP_ONLY_UNITS: &[&str] = &[
    "nC00",
];

/// Prologue01 map-object ids and the install-table unit they extend
/// (`war3map.w3u` custom object -> base id).
pub const PROLOGUE01_MAP_ONLY_BASE: &[(&str, &str)] = &[
    ("nC00", "ngst"),
];

/// Prologue01: item ids.
#[allow(dead_code)]
pub const PROLOGUE01_ITEMS: &[&str] = &[
    "phea", "phlt",
];

/// Prologue01: destructable ids carried by the map.
#[allow(dead_code)]
pub const PROLOGUE01_DESTRUCTABLES: &[&str] = &[
    "DTsp", "LTlt",
];

/// Map-local unit objects carried by `MAP_DEFS`: map stem, custom
/// rawcode, install-table base, the map's own resolved name.
#[allow(dead_code)]
pub const MAP_OBJECTS: &[(&str, &str, &str, &str)] = &[
    ("Prologue01", "nC00", "ngst", "Каменный голем"),
    ("Orc01", "nC08", "ncnt", "Хижина аборигена"),
    ("Orc01", "nC09", "nct1", "Хижина аборигена"),
    ("Orc01", "nC10", "nct2", "Хижина аборигена"),
    ("Orc01", "oC03", "otau", "Молодой минотавр"),
];
// <content-census:end>

mod complete {
    use super::*;

    fn sorted(mut ids: Vec<&str>) -> Vec<&str> {
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// `content.units.complete`: every unit id the four melee tech trees reach
    /// (`Builds=`/`Trains=`/`Sellunits=`/`Upgrade=` closure of the shipped
    /// profiles), every unit a shipped campaign map places, and every unit
    /// Orc01 places resolves to a real def.  Orc01 additionally needs the real
    /// model and the command-button icon: that is the acceptance row of
    /// `PACKETS-2026-11-18.md` for this lane.
    #[test]
    fn units() {
        let mut missing = Vec::new();
        for id in MELEE_UNITS
            .iter()
            .chain(CAMPAIGN_UNITS.iter())
            .chain(ORC01_UNITS.iter())
        {
            if data::def_of(id).is_none() {
                missing.push(*id);
            }
        }
        missing = sorted(missing);
        assert!(missing.is_empty(), "unit ids with no def: {missing:?}");

        let mut no_model = Vec::new();
        let mut no_icon = Vec::new();
        for id in ORC01_UNITS {
            let def = data::def_of(id).unwrap_or_else(|| panic!("Orc01 unit {id} has no def"));
            if !def.model_present {
                no_model.push(*id);
            }
            if def.icon.is_empty() {
                no_icon.push(*id);
            }
        }
        no_model = sorted(no_model);
        no_icon = sorted(no_icon);
        assert!(
            no_model.is_empty(),
            "Orc01 units whose model is absent from the archive: {no_model:?}"
        );
        assert!(
            no_icon.is_empty(),
            "Orc01 units without a command-button icon: {no_icon:?}"
        );
        println!(
            "content.units.complete melee={} campaign={} orc01={} orc01_map_only={}",
            MELEE_UNITS.len(),
            CAMPAIGN_UNITS.len(),
            ORC01_UNITS.len(),
            ORC01_MAP_ONLY_UNITS.len()
        );
    }

    /// `content.buildings.complete`: every building the melee tech tree reaches
    /// is a def with a real model, the worker build menu equals the
    /// worker-buildable set for every race, and every building def is reachable
    /// through the card, an `Upgrade=` chain or a placement ability.
    #[test]
    fn buildings() {
        assert!(!MELEE_BUILDINGS.is_empty(), "census carries no melee buildings");
        let mut missing = Vec::new();
        for id in MELEE_BUILDINGS {
            match data::def_of(id) {
                Some(def) => {
                    assert!(!def.model.is_empty(), "{id} has no model path");
                    assert!(def.model_present, "{id} model {} is absent", def.model);
                    assert!(def.is_building(), "{id} must be a building def");
                }
                None => missing.push(*id),
            }
        }
        missing = sorted(missing);
        assert!(missing.is_empty(), "melee buildings with no def: {missing:?}");

        for race in RACES {
            let menu = sorted(data::build_menu(race).iter().map(|def| def.id).collect());
            let placeable = sorted(
                data::worker_buildable(race)
                    .iter()
                    .map(|def| def.id)
                    .collect(),
            );
            assert_eq!(
                menu, placeable,
                "{race:?} build menu must equal the worker-buildable set"
            );
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
        // The two rows this census closed: the third human tower and the second
        // ziggurat tier are `Upgrade=` targets, so they are defs even though no
        // worker card lists them.
        for (tier, from) in [("hatw", "hwtw"), ("uzg2", "uzig")] {
            let def = data::def_of(tier).unwrap_or_else(|| panic!("{tier} missing"));
            assert_eq!(data::unit_upgrade_of(tier), Some(from), "{tier} upgrade path");
            assert!(!data::is_worker_buildable(def), "{tier} must not be on the card");
            assert!(def.model_present, "{tier} model {}", def.model);
        }
        println!(
            "content.buildings.complete melee_buildings={}",
            MELEE_BUILDINGS.len()
        );
    }

    /// `content.upgrades.complete`: our upgrade table is exactly the
    /// `UpgradeData.slk` row set, and every `Researches=` entry on a def names
    /// one of those rows.
    #[test]
    fn upgrades() {
        let ours = sorted(data::UPGRADES.iter().map(|upgrade| upgrade.id).collect());
        let source = sorted(SLK_UPGRADES.to_vec());
        assert_eq!(ours, source, "upgrade table must equal UpgradeData.slk");
        let mut dangling = Vec::new();
        for def in data::ROSTER.iter() {
            for upgrade in def.researches {
                if data::upgrade_def(upgrade).is_none() {
                    dangling.push(format!("{}: {upgrade}", def.id));
                }
            }
        }
        assert!(
            dangling.is_empty(),
            "researches with no UpgradeDef: {dangling:?}"
        );
        println!("content.upgrades.complete rows={}", ours.len());
    }

    /// `content.items.complete`: our item table is exactly the `ItemData.slk`
    /// row set, and every shop catalogue resolves to a real item.
    #[test]
    fn items() {
        let ours = sorted(data::ITEMS.iter().map(|item| item.id).collect());
        let source = sorted(SLK_ITEMS.to_vec());
        assert_eq!(ours, source, "item table must equal ItemData.slk");
        for def in data::ROSTER.iter() {
            for item in def.sells_items {
                assert!(
                    data::item_def(item).is_some(),
                    "{} sells unknown item {item}",
                    def.id
                );
            }
        }
        println!("content.items.complete rows={}", ours.len());
    }

    /// `content.abilities.complete`: the generic ability table is exactly the
    /// `AbilityData.slk` row set.
    #[test]
    fn abilities() {
        let ours = sorted(wc3_sim::abilities::data_ability_ids().collect());
        let source = sorted(SLK_ABILITIES.to_vec());
        assert_eq!(ours, source, "ability table must equal AbilityData.slk");
        println!("content.abilities.complete rows={}", ours.len());
    }

    /// `content.destructables.complete`: our destructable table is exactly the
    /// `DestructableData.slk` row set, so a map's trees/rocks always resolve.
    #[test]
    fn destructables() {
        let ours = sorted(data::DESTRUCTABLES.iter().map(|def| def.id).collect());
        let source = sorted(SLK_DESTRUCTABLES.to_vec());
        assert_eq!(
            ours, source,
            "destructable table must equal DestructableData.slk"
        );
        println!("content.destructables.complete rows={}", ours.len());
    }

    /// `content.orc01.complete`: the Orc01 acceptance census — every unit the
    /// chapter places resolves with a model and an icon, every item it creates
    /// is a real item row, and every destructable it carries is a real row.
    /// Map-local ids (`war3map.w3u`) are listed, never silently accepted.
    #[test]
    fn orc01() {
        let mut gaps = Vec::new();
        for id in ORC01_UNITS {
            match data::def_of(id) {
                Some(def) => {
                    if !def.model_present {
                        gaps.push(format!("{id}: model {}", def.model));
                    }
                    if def.icon.is_empty() {
                        gaps.push(format!("{id}: no icon"));
                    }
                }
                None => gaps.push(format!("{id}: no def")),
            }
        }
        for id in ORC01_ITEMS {
            if data::item_def(id).is_none() {
                gaps.push(format!("{id}: no item row"));
            }
        }
        for id in ORC01_DESTRUCTABLES {
            if data::destructable_def(id).is_none() {
                gaps.push(format!("{id}: no destructable row"));
            }
        }
        // Map-object ids (`war3map.w3u`) are custom objects of an install-table
        // unit.  Their content is complete when the base resolves and the map
        // does not override model/icon: the census generator parsed the object
        // file and asserts both (`ORC01_MAP_ONLY_BASE`).
        for (custom, base) in ORC01_MAP_ONLY_BASE {
            match data::def_of(base) {
                Some(def) => {
                    if !def.model_present {
                        gaps.push(format!("{custom}: base {base} model absent"));
                    }
                    if def.icon.is_empty() {
                        gaps.push(format!("{custom}: base {base} has no icon"));
                    }
                }
                None => gaps.push(format!("{custom}: base {base} has no def")),
            }
        }
        assert_eq!(
            ORC01_MAP_ONLY_BASE.len(),
            ORC01_MAP_ONLY_UNITS.len(),
            "every map-object id needs a base mapping"
        );
        assert!(gaps.is_empty(), "Orc01 content gaps: {gaps:?}");
        println!(
            "content.orc01.complete units={} items={} destructables={} map_only={:?}",
            ORC01_UNITS.len(),
            ORC01_ITEMS.len(),
            ORC01_DESTRUCTABLES.len(),
            ORC01_MAP_ONLY_UNITS
        );
    }
}
