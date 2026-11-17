//! TOP-8/9/10 parity: hero attributes/level/skills, buff/aura data and
//! `AbilityData`-generated spell numbers (`analysis/tech-lead/PARITY_MATRIX.md`
//! §7 rows 8, 9, 10).
//!
//! Expected values are read from the real RoC extraction; the comments quote the
//! sheet columns they come from.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::source::DirSource;
use wc3_data::{GameDatabase, PrimaryAttribute};

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn roc() -> &'static GameDatabase {
    static ROC: OnceLock<GameDatabase> = OnceLock::new();
    ROC.get_or_init(|| {
        GameDatabase::load(&DirSource::new(analysis_dir().join("war3_data"))).expect("load RoC")
    })
}

/// TOP-8: `UnitBalance.slk` hero block for `Hpal` / `Hamg`.
#[test]
fn hero_attributes_and_levels_come_from_unit_balance() {
    let data = roc();
    let paladin = data.hero("Hpal").expect("paladin hero");
    assert_eq!(paladin.level, 5, "UnitBalance.level");
    let attributes = paladin.attributes;
    assert_eq!(attributes.strength, 22, "STR");
    assert_eq!(attributes.intelligence, 17, "INT");
    assert_eq!(attributes.agility, 13, "AGI");
    assert!((attributes.strength_per_level - 2.7).abs() < 0.001, "STRplus");
    assert!((attributes.intelligence_per_level - 1.8).abs() < 0.001, "INTplus");
    assert!((attributes.agility_per_level - 1.5).abs() < 0.001, "AGIplus");
    assert_eq!(attributes.primary, Some(PrimaryAttribute::Strength), "Primary");

    // Growth is linear: level 5 = base + 4 * growth.
    let strength = attributes.value_at(PrimaryAttribute::Strength, 5);
    assert!((strength - (22.0 + 4.0 * 2.7)).abs() < 0.001, "STR at 5: {strength}");
    assert!((attributes.attack_bonus_at(5) - strength).abs() < 0.001, "primary bonus");

    let archmage = data.hero("Hamg").expect("archmage");
    assert_eq!(archmage.attributes.primary, Some(PrimaryAttribute::Intelligence));

    // Non-heroes are not heroes: `Primary` is absent / `_`.
    assert!(data.hero("hfoo").is_none(), "footman is not a hero");
    assert!(data.heroes().len() >= 12, "hero count: {}", data.heroes().len());
}

/// TOP-8: hero HP/mana/armour/damage per level are derived from the sheet, and
/// the derivation reproduces the sheet's own `realHP`/`realM` columns.
#[test]
fn hero_level_stats_reproduce_the_computed_columns() {
    let data = roc();
    let heroes = data.heroes();
    assert!(!heroes.is_empty(), "heroes discovered");
    let mut mismatches = Vec::new();
    for hero in &heroes {
        let unit = data.unit(&hero.id).expect("hero unit");
        let real_hp = unit.real_hp().ok().flatten();
        let real_mana = unit.real_mana().ok().flatten();
        // Level 1 must equal the sheet's computed columns exactly.
        if let Some(real_hp) = real_hp {
            if hero.hp_at(1) != real_hp {
                mismatches.push(format!("{}: hp_at(1)={} realHP={}", hero.id, hero.hp_at(1), real_hp));
            }
        }
        if let Some(real_mana) = real_mana.filter(|mana| *mana > 0) {
            if hero.mana_at(1) != real_mana {
                mismatches.push(format!(
                    "{}: mana_at(1)={} realM={}",
                    hero.id,
                    hero.mana_at(1),
                    real_mana
                ));
            }
        }
        // Growth: level 2 must be strictly higher when the attribute grows.
        assert!(hero.hp_at(2) >= hero.hp_at(1), "{} hp growth", hero.id);
        assert!(
            hero.armor_at(1) >= hero.attributes.base_armor as f32 - 0.001,
            "{} armour >= def",
            hero.id
        );
        assert!(hero.damage_bonus_at(1) > 0.0, "{} primary damage", hero.id);
    }
    assert!(mismatches.is_empty(), "derived stats diverge: {mismatches:?}");

    // Concrete sheet values (UnitBalance.slk): HP 100 + STR 22*25 = 650.
    let paladin = data.hero("Hpal").expect("paladin");
    assert_eq!(paladin.hp_at(1), 650, "realHP");
    assert_eq!(paladin.mana_at(1), 255, "realM = 17 INT * 15");
    assert!((paladin.attributes.hp_per_strength - 25.0).abs() < 0.001, "HP/STR");
    assert!(
        (paladin.attributes.mana_per_intelligence - 15.0).abs() < 0.001,
        "mana/INT"
    );
    // Armour: def 2 + AGI 13 * DefenseArmor 0.06 = 2.78.
    assert!((paladin.armor_at(1) - 2.78).abs() < 0.01, "armour {}", paladin.armor_at(1));
}

/// TOP-8: skill points and the level gate of each skill come from the data.
#[test]
fn hero_skill_points_and_level_gate() {
    let data = roc();
    let paladin = data.hero("Hpal").expect("paladin");
    assert_eq!(paladin.skill_points_at(1), 0, "no points at level 1");
    assert_eq!(paladin.skill_points_at(2), 1);
    assert_eq!(paladin.skill_points_at(5), 4);

    let learnable_at_1: Vec<&str> = paladin
        .skills_at_level(1)
        .iter()
        .map(|skill| skill.id.as_str())
        .collect();
    assert!(learnable_at_1.contains(&"AHhb"), "Holy Light at level 1");
    let all: Vec<&str> = paladin
        .skills_at_level(6)
        .iter()
        .map(|skill| skill.id.as_str())
        .collect();
    assert_eq!(all.len(), 4, "all four skills by level 6: {all:?}");
    // Skill levels are the data's `levels` column, not a guess.
    assert!(paladin.skills.iter().all(|skill| skill.levels >= 1));
    assert!(paladin
        .skills
        .iter()
        .all(|skill| skill.required_level.unwrap_or(1) >= 1));
}

/// TOP-8: skills come from `UnitAbilities.heroAbilList` + `AbilityData`.
#[test]
fn hero_skills_come_from_the_hero_ability_list() {
    let data = roc();
    let paladin = data.hero("Hpal").expect("paladin");
    let ids: Vec<&str> = paladin.skills.iter().map(|skill| skill.id.as_str()).collect();
    assert!(ids.contains(&"AHhb"), "Holy Light in heroAbilList: {ids:?}");
    assert!(ids.contains(&"AHds"), "Divine Shield: {ids:?}");
    assert_eq!(ids.len(), 4, "paladin has four skills: {ids:?}");

    let holy_light = paladin
        .skills
        .iter()
        .find(|skill| skill.id == "AHhb")
        .expect("holy light");
    // The RoC Russian install localises Holy Light as "Благодать".
    assert_eq!(holy_light.name.as_deref(), Some("Благодать"));
    assert_eq!(holy_light.levels, 3, "levels column");
    assert_eq!(holy_light.required_level, Some(1), "reqLevel");
    assert_eq!(holy_light.mana, Some(75), "Cost1");
    assert_eq!(holy_light.cooldown, Some(5.0), "Cool1");
    assert_eq!(holy_light.range, Some(800.0), "Rng1");
    assert!(holy_light.icon.is_some() && holy_light.hotkey.is_some());
}

/// TOP-8: XP curve and revive formula from `Units\MiscData.txt`.
#[test]
fn hero_rules_and_revive_costs_come_from_misc_data() {
    let data = roc();
    let rules = data.hero_rules().expect("hero rules");
    assert_eq!(rules.max_level, 10, "MaxHeroLevel");
    assert_eq!(rules.need_xp, [200, 500, 900, 1400, 2000, 2700, 3500, 4400, 5400, 5400]);
    assert_eq!(rules.grant_xp, [80, 100, 120, 140, 160, 200, 240, 280, 320, 400]);
    assert_eq!(rules.exp_range, 1200.0, "HeroExpRange");
    assert_eq!(rules.xp_to_next(1), Some(200));
    assert_eq!(rules.kill_xp(1), Some(80));

    let paladin = data.unit("Hpal").expect("paladin");
    let gold = paladin.gold_cost().ok().flatten().expect("gold cost") as i64;
    let build = paladin.build_time().ok().flatten().expect("build time") as i64;
    let level1 = data.hero_revive_cost("Hpal", 1).expect("revive cost");
    let expected = (gold as f32 * 0.40) as i64;
    assert_eq!(level1.gold, expected, "40% of {gold} at level 1");
    assert_eq!(level1.lumber, 0);
    assert!((level1.seconds - build as f32 * 0.5).abs() < 0.001, "time * level * 0.5");

    // Level 4: factor 0.40 + 3*0.10 = 0.70; time 4 * 0.5 = 2.0x.
    let level4 = data.hero_revive_cost("Hpal", 4).expect("revive cost");
    assert_eq!(level4.gold, (gold as f32 * 0.70) as i64);
    assert!((level4.seconds - build as f32 * 2.0).abs() < 0.001);
    // Cap at HeroMaxReviveCostGold.
    let capped = data.hero_revive_cost("Hpal", 9).expect("capped");
    assert!(capped.gold <= rules.max_revive_gold, "cap {}", rules.max_revive_gold);
}

/// TOP-9: auras are recognised from the AbilityData numbers alone.
#[test]
fn auras_are_derived_from_ability_numbers() {
    let data = roc();
    let devotion = data.ability_numbers("AHab").expect("devotion aura");
    assert!(devotion.is_aura(), "devotion aura detected");
    let aura = devotion.aura.as_ref().expect("aura numbers");
    assert_eq!(aura.radius, 900.0, "Area1");
    assert_eq!(aura.bonuses, vec![1.0], "Data11 = +1 armour at level 1");
    assert!(aura.targets.iter().any(|target| target == "friend"));
    assert!(aura.targets.iter().any(|target| target == "self"));
    assert_eq!(devotion.levels.len(), 3);
    // Passive: no mana, zero cooldown, zero duration.
    let level1 = devotion.first_level().expect("level 1");
    assert_eq!(level1.mana, None);
    assert_eq!(level1.cooldown, Some(0.0));
    assert_eq!(level1.duration, Some(0.0));

    let endurance = data.ability_numbers("AOae").expect("endurance aura");
    let aura = endurance.aura.as_ref().expect("aura");
    assert_eq!(aura.bonuses, vec![0.1, 0.05], "Data11/Data21 slots at level 1");
    assert_eq!(aura.bonus(1, 1), Some(0.1));
    assert_eq!(aura.bonus(2, 1), Some(0.2), "Data21");
    assert_eq!(aura.bonus(3, 1), Some(0.3), "Data31");

    // Devotion scales armour +1 per level: Data11/Data21/Data31 = 1,2,3.
    let devotion_aura = devotion.aura.as_ref().expect("aura");
    assert_eq!(devotion_aura.bonus(1, 1), Some(1.0));
    assert_eq!(devotion_aura.bonus(3, 1), Some(3.0));
    assert_eq!(devotion_aura.bonuses_by_level.len(), 3, "three levels");

    // An active spell is not an aura.
    assert!(!data.ability_numbers("AHbz").expect("blizzard").is_aura());
    assert!(!data.ability_numbers("AHhb").expect("holy light").is_aura());
    // The aura inventory is a real set, not a single row. RoC ships nine
    // passive support auras (Devotion/Endurance/Unholy/Vampiric/Thorns/
    // Trueshot/Brilliance/Command/…); the exact count is pinned so a predicate
    // regression cannot slip through.
    let auras = data.auras();
    let ids: Vec<&str> = auras.iter().map(|aura| aura.id.as_str()).collect();
    assert_eq!(auras.len(), 9, "RoC auras: {ids:?}");
    assert!(ids.contains(&"AHab"), "devotion in {ids:?}");
    assert!(ids.contains(&"AOae"), "endurance in {ids:?}");
}

/// TOP-9: buff definitions come from `war3map.w3b` overlays; retail ships no
/// base buff sheet, and the data layer says so instead of inventing rows.
#[test]
fn buff_store_reports_missing_base_sheet() {
    let data = roc();
    assert!(!data.buffs().is_base_available());
    assert_eq!(data.buffs().overlay_len(), 0);
}

/// TOP-10 gate (`PARITY_MATRIX.md` §7 row 10): melee-ID coverage.
///
/// Denominator = ability ids referenced by melee-race units
/// (`abilList` + `heroAbilList`); numerator = those AbilityData defines with
/// levels, i.e. `SpellDef` numbers can be generated from the sheet.
#[test]
fn melee_ability_coverage_clears_the_sixty_percent_gate() {
    let data = roc();
    let ids = data.melee_ability_ids();
    let (covered, total) = data.melee_ability_coverage();
    // Matrix baseline is 140 melee ids; the RoC extraction references 176.
    assert!(total >= 140, "melee ids: {total}");
    assert_eq!(total, ids.len(), "denominator is the melee id set");
    let ratio = covered as f64 / total as f64;
    assert!(
        ratio >= 0.60,
        "qa.abilities.coverage: {covered}/{total} ({ratio:.3}) < 0.60"
    );
    // The example abilities a melee player actually sees must be covered.
    for id in ["AHhb", "AHbz", "Ahea", "AOcl", "AUdc", "Adef"] {
        assert!(
            data.ability_numbers(id)
                .is_some_and(|numbers| !numbers.levels.is_empty()),
            "{id} must be generated from AbilityData"
        );
    }
}

/// TOP-10: every editor ability with levels yields generated spell numbers.
#[test]
fn spell_numbers_cover_the_ability_sheet() {
    let data = roc();
    let (covered, total) = data.ability_coverage();
    assert!(total > 300, "editor abilities: {total}");
    let ratio = covered as f64 / total as f64;
    assert!(
        ratio >= 0.60,
        "qa.abilities.coverage needs >= 60%: {covered}/{total} ({ratio:.3})"
    );

    let blizzard = data.ability_numbers("AHbz").expect("blizzard");
    assert_eq!(blizzard.levels.len(), 3, "levels 1-3");
    let level1 = blizzard.first_level().expect("level 1");
    assert_eq!(level1.mana, Some(75), "Cost1");
    assert_eq!(level1.cooldown, Some(6.0), "Cool1");
    assert_eq!(level1.area, Some(200.0), "Area1");
    assert_eq!(level1.range, Some(800.0), "Rng1");
    assert_eq!(level1.slot(1), Some(6.0), "Data11 waves");
    assert_eq!(level1.slot(2), Some(30.0), "Data12 damage");
    assert_eq!(level1.slot(3), Some(6.0), "Data13 targets");
    assert_eq!(level1.slot(4), Some(0.5), "Data14 interval");
    // Level 2/3 escalate from their own columns.
    assert_eq!(blizzard.levels[1].mana, Some(75));
    assert_eq!(blizzard.levels[1].area, Some(225.0));
    assert_eq!(blizzard.levels[2].slot(2), Some(50.0));
}

/// TOP-10: the eleven hand-written sim spells are a subset of the generated set.
#[test]
fn the_eleven_hand_written_spells_exist_in_the_generated_set() {
    let data = roc();
    let hand_written = [
        "AHhb", "AHds", "AHbz", "Ahea", "AEer", "AOcl", "AHtb", "AHtc", "AUdc", "AUfn", "AUcs",
    ];
    let mut missing = Vec::new();
    for id in hand_written {
        match data.ability_numbers(id) {
            Some(numbers) => {
                assert!(!numbers.levels.is_empty(), "{id} levels");
                assert!(numbers.name.is_some(), "{id} name");
            }
            None => missing.push(id),
        }
    }
    assert!(missing.is_empty(), "hand-written spells absent from AbilityData: {missing:?}");
}
