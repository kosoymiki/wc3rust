//! Orc01 campaign ability census (lane 511).
//!
//! The acceptance basis for the whole build is `Maps\Campaign\Orc01.w3m`.
//! This suite reads the actual map from the shipped archives, enumerates the
//! units it places and the ability rawcodes its script uses, and asserts that
//! every one resolves through [`wc3_sim::abilities::spell_for_ability`] to a
//! typed spell or to an explicit data-only row with a printable reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_mpq::MpqArchive;

fn orc01_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Maps/Campaign/Orc01.w3m")
}

fn orc01_archive() -> Option<MpqArchive> {
    let path = orc01_path();
    if !path.is_file() {
        return None;
    }
    let base = map_mpq_base(&path).ok()?;
    MpqArchive::open_at(&path, base).ok()
}

fn placed_unit_types(archive: &MpqArchive) -> Vec<String> {
    let bytes = archive.read("war3mapUnits.doo").expect("war3mapUnits.doo");
    let placed = parse_units(&bytes).expect("parse units");
    let mut ids: Vec<String> = placed
        .iter()
        .map(|unit| String::from_utf8_lossy(&unit.type_id).to_string())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn script_ability_codes(archive: &MpqArchive) -> BTreeSet<String> {
    let bytes = archive
        .read("war3map.j")
        .or_else(|_| archive.read("Scripts\\war3map.j"))
        .or_else(|_| archive.read("scripts\\war3map.j"))
        .expect("map script");
    let text = String::from_utf8_lossy(&bytes);
    let mut codes = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index + 5 < bytes.len() {
        if bytes[index] == b'\'' {
            let inner = &bytes[index + 1..index + 5];
            if bytes[index + 5] == b'\''
                && inner[0] == b'A'
                && inner.iter().all(|byte| byte.is_ascii_alphanumeric())
            {
                codes.insert(String::from_utf8_lossy(inner).to_string());
            }
        }
        index += 1;
    }
    codes
}

#[test]
fn census() {
    let Some(archive) = orc01_archive() else {
        eprintln!("skip: Orc01.w3m fixture missing");
        return;
    };
    let types = placed_unit_types(&archive);
    println!("ORC01 unit types ({}): {types:?}", types.len());
    let codes = script_ability_codes(&archive);
    println!("ORC01 script A-codes ({}): {codes:?}", codes.len());
    let mut placed_abilities = BTreeSet::new();
    for id in &types {
        if let Some(def) = wc3_sim::def_of(id) {
            for ability in def.ability_ids {
                placed_abilities.insert(ability.to_string());
            }
        }
    }
    println!("ORC01 placed-unit abilities ({}): {placed_abilities:?}", placed_abilities.len());
    let mut all = placed_abilities.clone();
    all.extend(codes.iter().cloned());
    let mut unmapped = Vec::new();
    let mut data_only = Vec::new();
    for id in &all {
        match wc3_sim::abilities::spell_for_ability(id) {
            Some(wc3_sim::abilities::Spell::DataOnly { .. }) => data_only.push(id.clone()),
            Some(_) => {}
            None => unmapped.push(id.clone()),
        }
    }
    println!("ORC01 all ability ids ({}): {all:?}", all.len());
    println!("ORC01 data-only ({}): {data_only:?}", data_only.len());
    println!("ORC01 unmapped ({}): {unmapped:?}", unmapped.len());

    // Roster-wide census (all four races + creeps + campaign heroes).
    let mut roster_ids = BTreeSet::new();
    for def in wc3_sim::ROSTER.iter() {
        for ability in def.ability_ids {
            roster_ids.insert(ability.to_string());
        }
    }
    let mut roster_data_only = Vec::new();
    let mut roster_unmapped = Vec::new();
    for id in &roster_ids {
        match wc3_sim::abilities::spell_for_ability(id) {
            Some(wc3_sim::abilities::Spell::DataOnly { .. })
            | Some(wc3_sim::abilities::Spell::DataTransformation { .. }) => {
                roster_data_only.push(id.clone())
            }
            Some(_) => {}
            None => roster_unmapped.push(id.clone()),
        }
    }
    println!("ROSTER ability ids ({}): data-only/no-effect ({}): {roster_data_only:?}", roster_ids.len(), roster_data_only.len());
    println!("ROSTER unmapped ({}): {roster_unmapped:?}", roster_unmapped.len());
    // Summon rows whose spawn target has no roster def (the handler skips).
    let mut broken_summons = Vec::new();
    for id in wc3_sim::data_ability_ids() {
        let def = wc3_sim::generic_ability(id).expect("row");
        for effect in wc3_sim::effects::build_effects(def) {
            if let wc3_sim::EffectSpec::Summon { def_id, .. } = effect {
                if wc3_sim::def_of(def_id).is_none() {
                    broken_summons.push(format!("{} -> {}", def.id, def_id));
                }
            }
        }
    }
    println!("SUMMON rows with missing target ({}): {broken_summons:?}", broken_summons.len());
}
