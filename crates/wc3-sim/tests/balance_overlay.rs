//! mechanic `balance.overlay.applies`: the Reforged 2.x overlay
//! (`analysis/balance/reforged-2.x.json`) reaches the generated roster, every
//! applied row matches its def field, and no row is orphaned.

use std::collections::HashSet;

use wc3_sim::UnitDef;
use wc3_sim::data::{ABILITY_OVERLAY, ROSTER, ROSTER_OVERLAY, UPGRADE_OVERLAY, def_of};

fn field_value(def: &UnitDef, field: &str) -> Option<f64> {
    Some(match field {
        "stats.hp" => def.stats.hp as f64,
        "stats.armor" => def.stats.armor as f64,
        "stats.damage" => def.stats.damage as f64,
        "stats.range" => def.stats.range as f64,
        "stats.speed" => def.stats.speed as f64,
        "stats.cost" => def.stats.cost as f64,
        "stats.lumber_cost" => def.stats.lumber_cost as f64,
        "stats.build_ticks" => def.stats.build_ticks as f64,
        "stats.train_ticks" => def.stats.train_ticks as f64,
        "stats.hero_strength" => def.stats.hero_strength as f64,
        "stats.hero_agility" => def.stats.hero_agility as f64,
        "stats.hero_intelligence" => def.stats.hero_intelligence as f64,
        "stats.hero_strength_gain" => def.stats.hero_strength_gain as f64,
        "stats.hero_agility_gain" => def.stats.hero_agility_gain as f64,
        "stats.hero_intelligence_gain" => def.stats.hero_intelligence_gain as f64,
        _ => return None,
    })
}

#[test]
fn balance_overlay_applies() {
    // Three values that only exist because the overlay was applied:
    // Archer 260 -> 275 and Keeper strength gain 1.8 -> 2.0 (2.0.4 / 2.0.2),
    // Huntress 350 -> 340 (2.0.2).
    assert_eq!(def_of("earc").expect("Archer").stats.hp, 275);
    assert_eq!(
        def_of("Ekee").expect("Keeper").stats.hero_strength_gain,
        2.0
    );
    assert_eq!(def_of("esen").expect("Huntress").stats.speed, 340.0);
    // The same values are visible in the generated table.
    assert!(ROSTER_OVERLAY.iter().any(|row| {
        row.id == "earc" && row.field == "stats.hp" && row.reforged == 275.0
    }));
    assert!(ROSTER_OVERLAY.iter().any(|row| {
        row.id == "Ekee" && row.field == "stats.hero_strength_gain" && row.reforged == 2.0
    }));
    assert!(ROSTER_OVERLAY.iter().any(|row| {
        row.id == "esen" && row.field == "stats.speed" && row.reforged == 340.0
    }));

    // No roster row is orphaned: every row names a real def and the def field
    // really carries the Reforged number.
    for row in ROSTER_OVERLAY {
        let def = def_of(row.id)
            .unwrap_or_else(|| panic!("orphaned roster overlay row: {} {}", row.id, row.field));
        let value = field_value(def, row.field).unwrap_or_else(|| {
            panic!("orphaned roster overlay field: {}.{}", row.id, row.field)
        });
        assert!(
            (value - row.reforged).abs() < 1e-6,
            "{}.{} is {value}, overlay says {}",
            row.id,
            row.field,
            row.reforged
        );
        assert!(!row.source.is_empty(), "{}.{} has no source", row.id, row.field);
    }

    // No ability row is orphaned: the id is either in a roster def's ability
    // list or implemented as a SpellDef on a roster def.
    let mut ability_ids: HashSet<&'static str> = HashSet::new();
    for def in ROSTER {
        for id in def.ability_ids {
            ability_ids.insert(*id);
        }
        for spell in def.spells {
            ability_ids.insert(spell.def().id);
        }
    }
    for row in ABILITY_OVERLAY {
        assert!(
            ability_ids.contains(row.id),
            "orphaned ability overlay row: {}.{}",
            row.id,
            row.field
        );
        assert!(!row.source.is_empty(), "{}.{} has no source", row.id, row.field);
    }

    // No upgrade row is orphaned: every id is a real UpgradeDef in the
    // generated upgrade table (the generator checked the field baseline).
    let upgrades = include_str!("../src/upgrades_gen.rs");
    for row in UPGRADE_OVERLAY {
        assert!(
            upgrades.contains(&format!("id: \"{}\"", row.id)),
            "orphaned upgrade overlay row: {}.{}",
            row.id,
            row.field
        );
        assert!(!row.source.is_empty(), "{}.{} has no source", row.id, row.field);
    }

    assert!(!ROSTER_OVERLAY.is_empty());
    assert!(!ABILITY_OVERLAY.is_empty());
    assert!(!UPGRADE_OVERLAY.is_empty());
}
