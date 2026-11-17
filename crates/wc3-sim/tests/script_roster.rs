//! `jass.script-roster` — every unit id the 96-map script corpus creates with
//! `CreateUnit`/`CreateUnitAtLoc`/`CreateCorpse` resolves through `data::def_of`.
//!
//! A `CreateUnit` for an id the roster does not know returns null in the
//! simulation, so the script's unit never appears. This test is the gate: it
//! scans the extracted scripts, resolves every literal rawcode, and fails when a
//! new id appears that neither the tables nor the documented `UNREACHABLE` list
//! covers.
//!
//! `UNREACHABLE` is evidence-backed, not a place to hide ids:
//! * `nfro`, `nrac`, `necr`, `nfly`, `nfed` — no `UnitData.slk` /
//!   `UnitBalance.slk` / `UnitWeapons.slk` row in War3.mpq (RoC) or War3x.mpq
//!   (1.31). Only `Units\UnitUI.slk` still lists them (editor UI residue), and
//!   `nrac` is a map-only unit: `(2)Justice.w3x` spawns it from `war3mapUnits.doo`
//!   although no shipped table defines it. There is no stat row to import, so
//!   inventing one would be fabrication.
//! * `o000`, `o002`, `o003`, `o006` — custom rawcodes of the map
//!   "Narabotka ot Tokhi"; they exist only in that map's `war3map.w3u` object
//!   data, not in War3/War3x tables.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Ids the corpus creates that have no row in the shipped unit tables.
const UNREACHABLE: &[(&str, &str)] = &[
    ("nfro", "no UnitData/UnitBalance row in War3.mpq or War3x.mpq"),
    ("nrac", "map-only unit of (2)Justice.w3x; no shipped stat row"),
    ("necr", "no UnitData/UnitBalance row in War3.mpq or War3x.mpq"),
    ("nfly", "no UnitData/UnitBalance row in War3.mpq or War3x.mpq"),
    ("nfed", "no UnitData/UnitBalance row in War3.mpq or War3x.mpq"),
    ("o000", "custom object of Narabotka ot Tokhi (war3map.w3u)"),
    ("o002", "custom object of Narabotka ot Tokhi (war3map.w3u)"),
    ("o003", "custom object of Narabotka ot Tokhi (war3map.w3u)"),
    ("o006", "custom object of Narabotka ot Tokhi (war3map.w3u)"),
];

fn corpus_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-jass/extracted");
    dir.is_dir().then_some(dir)
}

/// Rawcodes passed as the unit-type argument of the create-unit natives.
fn created_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for call in [
        "CreateUnit(",
        "CreateUnitAtLoc(",
        "CreateUnitAtLocSaveLast(",
        "CreateCorpse(",
    ] {
        let mut rest = text;
        while let Some(open) = rest.find(call) {
            let after = &rest[open + call.len()..];
            let Some(close) = after.find(')') else { break };
            let args = &after[..close];
            // The unit id is the first four-character literal in the call.
            let mut chars = args.char_indices().peekable();
            while let Some((index, quote)) = chars.next() {
                if quote != '\'' {
                    continue;
                }
                if let Some(end) = args[index + 1..].find('\'') {
                    let raw = &args[index + 1..index + 1 + end];
                    if raw.chars().count() == 4 {
                        ids.push(raw.to_string());
                    }
                }
                break;
            }
            rest = after;
        }
    }
    ids
}

#[test]
fn jass_script_created_unit_ids_resolve() {
    let Some(dir) = corpus_dir() else {
        eprintln!("skip: analysis/agent-jass/extracted missing");
        return;
    };
    let mut files = 0usize;
    let mut calls: BTreeMap<String, usize> = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("j") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for id in created_ids(&text) {
            *calls.entry(id).or_default() += 1;
        }
    }
    assert!(files >= 90, "the corpus is the 96 extracted scripts, saw {files}");
    let unreachable: BTreeSet<&str> = UNREACHABLE.iter().map(|(id, _)| *id).collect();
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    for id in calls.keys() {
        if wc3_sim::def_of(id).is_some() {
            resolved.push(id.as_str());
        } else if !unreachable.contains(id.as_str()) {
            missing.push(id.as_str());
        }
    }
    eprintln!(
        "SCRIPT_ROSTER files={files} distinct_ids={} resolved={} unreachable={} calls={}",
        calls.len(),
        resolved.len(),
        calls.len() - resolved.len(),
        calls.values().sum::<usize>()
    );
    assert!(
        missing.is_empty(),
        "script-created ids with no def and no documented reason: {missing:?}"
    );
    // The demon-race creeps the corpus creates must resolve through CREEP_DEFS.
    for id in ["nbal", "nfel", "ninf"] {
        assert!(
            wc3_sim::def_of(id).is_some(),
            "{id} is created by the BanewoodBog / Stranglethorn scripts and must have a def"
        );
    }
    // Every documented-unreachable id must actually be unresolved, so the list
    // cannot silently rot into a stale excuse.
    for (id, reason) in UNREACHABLE {
        assert!(
            wc3_sim::def_of(id).is_none(),
            "{id} now resolves; delete it from UNREACHABLE ({reason})"
        );
    }
}

#[test]
fn jass_demon_creeps_carry_real_table_stats() {
    // Doom Guard / Felhound / Infernal: hp, damage and speed come from the
    // shipped War3x tables, not from a defaulted row.
    for (id, hp) in [("nbal", 1350), ("nfel", 750), ("ninf", 1500)] {
        let def = wc3_sim::def_of(id).expect(id);
        assert_eq!(def.stats.hp, hp, "{id} hit points from UnitBalance.slk");
        assert!(def.stats.damage > 0, "{id} damage from UnitWeapons.slk");
        assert!(def.stats.speed > 0.0, "{id} speed from UnitBalance.slk");
    }
}
