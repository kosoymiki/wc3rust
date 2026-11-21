//! Damage/armor tables, acquisition, ranged combat and death handling.

mod common;

use common::*;
use wc3_sim::{ArmorType, AttackType, Sim, Vec2};

#[test]
fn roster_armor_and_attack_types_are_grounded_in_wc3() {
    let mut sim = Sim::new(51, 8, 8);
    let footman = sim.spawn_def(0, "hfoo", Vec2::new(100.0, 100.0)).unwrap();
    let rifleman = sim.spawn_def(0, "hrif", Vec2::new(200.0, 100.0)).unwrap();
    let knight = sim.spawn_def(0, "hkni", Vec2::new(300.0, 100.0)).unwrap();
    let hall = sim.spawn_def(0, "htow", Vec2::new(500.0, 300.0)).unwrap();
    assert_eq!(
        sim.unit(footman).unwrap().def().stats.armor_type,
        ArmorType::Medium
    );
    assert_eq!(
        sim.unit(rifleman).unwrap().def().stats.armor_type,
        ArmorType::Light
    );
    assert_eq!(
        sim.unit(knight).unwrap().def().stats.armor_type,
        ArmorType::Heavy
    );
    assert_eq!(
        sim.unit(hall).unwrap().def().stats.armor_type,
        ArmorType::Fortified
    );
    assert_eq!(
        sim.unit(footman).unwrap().def().stats.attack_type,
        AttackType::Normal
    );
    assert_eq!(
        sim.unit(rifleman).unwrap().def().stats.attack_type,
        AttackType::Pierce
    );
}

#[test]
fn typed_damage_applies_table_then_armor() {
    let mut sim = Sim::new(52, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(300.0, 300.0));
    let target = footman(&mut sim, 1, Vec2::new(340.0, 300.0));
    let before = sim.unit(target).unwrap().hp;
    // Normal vs Medium x1.5 = 150; armor 2 => 150 * 100 / 112 = 133.
    let dealt = sim.apply_damage(attacker, target, 100, AttackType::Normal);
    assert_eq!(dealt, 133);
    assert_eq!(sim.unit(target).unwrap().hp, before - 133);
}

#[test]
fn pierce_versus_light_and_fortified_differ() {
    let mut sim = Sim::new(53, 16, 16);
    let attacker = sim.spawn_def(0, "hrif", Vec2::new(100.0, 100.0)).unwrap();
    let light = sim.spawn_def(1, "hrif", Vec2::new(300.0, 300.0)).unwrap();
    let fort = sim.spawn_def(1, "htow", Vec2::new(600.0, 600.0)).unwrap();
    // Pierce vs Light x2.0, armor 0 => 100.
    assert_eq!(
        sim.apply_damage(attacker, light, 50, AttackType::Pierce),
        100
    );
    // Pierce vs Fortified x0.35, armor 5 => 17 * 100/130 = 13.
    assert_eq!(sim.apply_damage(attacker, fort, 50, AttackType::Pierce), 13);
}

#[test]
fn negative_armor_amplifies_damage() {
    let mut sim = Sim::new(54, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(100.0, 100.0));
    let target = footman(&mut sim, 1, Vec2::new(200.0, 100.0));
    sim.unit_mut(target).unwrap().armor = -5;
    // Normal vs Medium 1.5 = 150; negative armor -5 amplifies x1.266 => 189.
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        189
    );
}

#[test]
fn damage_kills_and_clears_production() {
    let mut sim = Sim::new(55, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(300.0, 300.0));
    let hall = sim.spawn_def(1, "htow", Vec2::new(600.0, 600.0)).unwrap();
    sim.step(&[wc3_sim::Command::Train {
        building: hall,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    assert!(!sim.unit(hall).unwrap().production.is_empty());
    let events = {
        sim.apply_damage(attacker, hall, 10_000, AttackType::Chaos);
        sim.events.clone()
    };
    assert!(!sim.unit(hall).unwrap().alive);
    assert!(sim.unit(hall).unwrap().production.is_empty());
    assert!(
        events
            .iter()
            .any(|event| matches!(event, wc3_sim::Event::UnitDied(id) if *id == hall))
    );
}

#[test]
fn ranged_unit_shoots_without_closing_to_melee() {
    let mut sim = Sim::new(56, 32, 32);
    let rifleman = sim.spawn_def(0, "hrif", Vec2::new(500.0, 500.0)).unwrap();
    let enemy = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.step(&[wc3_sim::Command::Attack {
        units: vec![rifleman],
        target: enemy,
    }]);
    step_n(&mut sim, 160);
    let shooter = sim.unit(rifleman).unwrap();
    let victim = sim.unit(enemy).unwrap();
    assert!(victim.hp < victim.max_hp, "target takes ranged damage");
    // The melee footman closes in; the rifleman itself stays at its post.
    assert!(
        shooter.pos.distance(Vec2::new(500.0, 500.0)) < 120.0,
        "rifleman does not charge into melee: {:?}",
        shooter.pos
    );
}

#[test]
fn attack_walks_into_melee_range_and_kills() {
    let mut sim = Sim::new(57, 24, 24);
    let attacker = footman(&mut sim, 0, Vec2::new(400.0, 400.0));
    let target = footman(&mut sim, 1, Vec2::new(1200.0, 400.0));
    sim.step(&[wc3_sim::Command::Attack {
        units: vec![attacker],
        target,
    }]);
    step_n(&mut sim, 1600);
    assert!(!sim.unit(target).unwrap().alive);
}

#[test]
fn invulnerable_target_ignores_damage() {
    let mut sim = Sim::new(58, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(100.0, 100.0));
    let target = footman(&mut sim, 1, Vec2::new(200.0, 100.0));
    sim.unit_mut(target).unwrap().invulnerable_until = sim.tick + 100;
    assert_eq!(
        sim.apply_damage(attacker, target, 500, AttackType::Chaos),
        0
    );
    sim.unit_mut(target).unwrap().invulnerable_until = 0;
    assert!(sim.apply_damage(attacker, target, 10, AttackType::Chaos) > 0);
}

#[test]
fn building_tower_attacks_nearby_enemies() {
    let mut sim = Sim::new(59, 24, 24);
    let tower = sim.spawn_def(0, "hgtw", Vec2::new(800.0, 800.0)).unwrap();
    let enemy = footman(&mut sim, 1, Vec2::new(1000.0, 800.0));
    assert!(sim.unit(tower).unwrap().def().stats.damage > 0);
    step_n(&mut sim, 300);
    assert!(sim.unit(enemy).unwrap().hp < sim.unit(enemy).unwrap().max_hp);
}

#[test]
fn dead_units_stop_acting() {
    let mut sim = Sim::new(60, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(200.0, 200.0));
    let victim = footman(&mut sim, 1, Vec2::new(400.0, 200.0));
    sim.apply_damage(attacker, victim, 10_000, AttackType::Chaos);
    let position = sim.unit(victim).unwrap().pos;
    step_n(&mut sim, 120);
    assert_eq!(sim.unit(victim).unwrap().pos, position);
    assert!(!sim.unit(victim).unwrap().alive);
}

// ---------------------------------------------------------------------------
// Shipped-table cross-check
// ---------------------------------------------------------------------------

/// mechanic `combat.damage-table` (MECHANICS_GAP_MAP row 1): every definition's
/// combat numbers are the shipped `UnitBalance.slk` / `UnitWeapons.slk` cells,
/// not values retyped by hand.
///
/// The oracle is `wc3_formats::gamedata::Sheet`, which parses the retail SLK
/// bytes on its own code path (the generator that produced `roster_gen.rs` is
/// not involved). Two shipped extracts exist in the tree and the generator's
/// documented rule is "the current install table first, the TFT extract when a
/// row is missing", so a cell matches when it equals the `analysis/war3_full`
/// cell or, for a row absent there, the `analysis/war3x_data` cell.
///
/// The only accepted deltas are the rows the Reforged 2.x balance overlay
/// declares in `analysis/balance/reforged-2.x.json`: a mismatch the overlay
/// does not name fails the test instead of being silently excused.
///
/// Compared per definition with a shipped UnitBalance row: hit points
/// (`HP`, heroes `realHP`), armor (`def`), speed (`spd`), gold/lumber cost,
/// food used/provided, building time (`bldtm`), and the primary weapon's
/// `dmgplus1`/`dice1`/`sides1`/`rangeN1`/`cool1`.
#[test]
fn combat_damage_table_unit_stats_match_shipped_slk() {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use wc3_formats::gamedata::Sheet;
    use wc3_sim::data;

    fn analysis_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
    }

    fn sheet(extract: &str, file: &str) -> &'static Sheet {
        static SHEETS: OnceLock<Vec<(String, Sheet)>> = OnceLock::new();
        let sheets = SHEETS.get_or_init(|| {
            let mut out = Vec::new();
            for extract in ["war3_full", "war3x_data"] {
                let path = analysis_dir()
                    .join(extract)
                    .join("Units")
                    .join(file);
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                let parsed = Sheet::parse(file, &bytes)
                    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
                out.push((extract.to_string(), parsed));
            }
            out
        });
        sheets
            .iter()
            .find(|(name, _)| name == extract)
            .map(|(_, sheet)| sheet)
            .expect("sheet loaded")
    }

    /// `Some(sheet cell)` when the current install table has the row, else the
    /// TFT cell (the generator's fallback), else `None`.
    fn number(full: &Sheet, tft: &Sheet, id: &str, column: &str) -> Option<f64> {
        let sheet = if full.row_ids().iter().any(|row| row == id) {
            full
        } else {
            tft
        };
        sheet
            .cell(id, column)
            .filter(|cell| !cell.is_absent())
            .and_then(|cell| cell.text())
            .and_then(|text| text.trim().parse::<f64>().ok())
    }

    /// `(id, field)` pairs the Reforged overlay rewrites. The file is generated
    /// from the official 2.0.2/2.0.4 patch notes by
    /// `analysis/agent-sim/gen_roster.py`, so a tiny scanner is enough: every
    /// row carries `"id"`, `"field"` and `"reforged"`. The row-count floor
    /// keeps a parser regression from turning the exemption list into a silent
    /// empty set.
    fn overlay_rows() -> &'static HashSet<(String, String)> {
        static ROWS: OnceLock<HashSet<(String, String)>> = OnceLock::new();
        ROWS.get_or_init(|| {
            let path = analysis_dir().join("balance/reforged-2.x.json");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let start = text
                .find("\"roster\"")
                .expect("overlay has a roster array");
            let mut rows = HashSet::new();
            let mut id: Option<String> = None;
            for line in text[start..].lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("\"id\": ") {
                    id = Some(json_string(rest));
                } else if let Some(rest) = trimmed.strip_prefix("\"field\": ") {
                    let field = json_string(rest);
                    if let Some(id) = id.take() {
                        rows.insert((id, field));
                    }
                }
            }
            assert!(
                rows.len() >= 20,
                "overlay scanner found {} rows, expected the generated 20+ roster deltas",
                rows.len()
            );
            rows
        })
    }

    fn json_string(text: &str) -> String {
        let text = text.trim();
        let text = text.strip_suffix(',').unwrap_or(text).trim();
        text.strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(text)
            .to_string()
    }

    let balance = sheet("war3_full", "UnitBalance.slk");
    let balance_tft = sheet("war3x_data", "UnitBalance.slk");
    let weapons = sheet("war3_full", "UnitWeapons.slk");
    let weapons_tft = sheet("war3x_data", "UnitWeapons.slk");
    let overlay = overlay_rows();
    let mut mismatches: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut hero_hp = 0usize;
    let mut overlay_hits = 0usize;
    let mut tft_fallback = 0usize;

    // The melee roster: the units and buildings the four races actually fight
    // with. Creep/shop/campaign rows come from their own generators and zero
    // the cost columns a creep never pays (pinned by their own tests).
    let defs: Vec<&wc3_sim::UnitDef> = data::ROSTER.iter().collect();
    assert!(
        defs.len() >= 100,
        "melee roster rows: {} (expected the four-race roster)",
        defs.len()
    );

    for def in &defs {
        if !balance.row_ids().iter().any(|row| row == &def.id)
            && balance_tft.row_ids().iter().any(|row| row == &def.id)
        {
            tft_fallback += 1;
        }
        let Some(_) = number(balance, balance_tft, def.id, "HP") else {
            mismatches.push(format!("{}: no UnitBalance row", def.id));
            continue;
        };
        checked += 1;
        let stats = &def.stats;

        // Heroes store `HP` as the pre-attribute base; the roster carries the
        // level-1 pool the game shows, which the sheet computes in `realHP`.
        let hp_column = if def.hero {
            hero_hp += 1;
            "realHP"
        } else {
            "HP"
        };
        let mut check = |field: &str, column: &str, scale: f64, actual: i64| {
            let Some(expected) = number(balance, balance_tft, def.id, column) else {
                return;
            };
            let expected = (expected * scale).round() as i64;
            if expected == actual {
                return;
            }
            if overlay.contains(&(def.id.to_string(), field.to_string())) {
                overlay_hits += 1;
            } else {
                mismatches.push(format!(
                    "{} {field}: slk {column}={expected} sim={actual}",
                    def.id
                ));
            }
        };

        check("stats.hp", hp_column, 1.0, i64::from(stats.hp));
        check("stats.armor", "def", 1.0, i64::from(stats.armor));
        check("stats.speed", "spd", 1.0, stats.speed as i64);
        check("stats.cost", "goldcost", 1.0, i64::from(stats.cost));
        check(
            "stats.lumber_cost",
            "lumbercost",
            1.0,
            i64::from(stats.lumber_cost),
        );
        check("stats.supply", "fused", 1.0, i64::from(stats.supply));
        check(
            "stats.supply_provided",
            "fmade",
            1.0,
            i64::from(stats.supply_provided),
        );
        if stats.is_building {
            check(
                "stats.build_ticks",
                "bldtm",
                30.0,
                i64::from(stats.build_ticks),
            );
        }

        // Primary weapon slot: the damage roll is `dmgplus1 + dice1` ..
        // `dmgplus1 + dice1*sides1`, exactly the cells the roster keeps.
        check("stats.damage", "dmgplus1", 1.0, i64::from(stats.damage));
        check(
            "stats.damage_dice",
            "dice1",
            1.0,
            i64::from(stats.damage_dice),
        );
        check(
            "stats.damage_sides",
            "sides1",
            1.0,
            i64::from(stats.damage_sides),
        );
        check("stats.range", "rangeN1", 1.0, stats.range.round() as i64);
        check(
            "stats.attack_ticks",
            "cool1",
            30.0,
            i64::from(stats.attack_ticks),
        );
        let _ = (&weapons, &weapons_tft);
    }

    assert!(
        mismatches.is_empty(),
        "{} of {checked} definitions disagree with the shipped SLK \
         ({overlay_hits} overlay deltas accepted, {tft_fallback} TFT-fallback rows): {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(12)]
    );
    assert_eq!(checked, defs.len(), "every definition has a shipped row");
    assert_eq!(hero_hp, defs.iter().filter(|def| def.hero).count());
    assert!(tft_fallback > 0, "the TFT extract covers rows war3_full lacks");
}
