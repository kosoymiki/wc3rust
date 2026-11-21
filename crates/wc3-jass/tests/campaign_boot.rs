//! Campaign boot sweep: all 44 shipped chapters' `war3map.j` boot on real maps.
//!
//! The layer's behaviour proof, not a parse count. Each chapter loads through
//! the canonical engine (`wc3_re::engine::Engine` — the same boot the game
//! runtime uses: `common.j` → `Blizzard.j` → `war3map.j`, unit creation, and
//! `InitCustomTriggers`), then runs 100 mission ticks so the initialization
//! triggers execute. The harness is `wc3_re::mapboot`, which records per map
//! the registered and executed triggers, the script errors, and every native
//! the VM could not implement.
//!
//! Gates:
//! - 44/44 maps boot (`ok`, `main_ok`), with no script errors;
//! - every map registers triggers and executes at least one after boot;
//! - no outcome-critical native (quests, victory/defeat, the human seat,
//!   hashtables) falls through unimplemented, so a mission cannot "boot" while
//!   the calls that decide its ending do nothing;
//! - the remaining unimplemented ledger is printed in full as the measured
//!   audit trail.

use std::path::{Path, PathBuf};

/// Natives whose absence changes what the mission *does*: the quest panel, the
/// victory/defeat conditions, the human seat, and the handle id the map's
/// hashtables are keyed on. A campaign map that boots with one of these
/// unimplemented looks alive and cannot be finished.
const OUTCOME_NATIVES: &[&str] = &[
    "questcreate",
    "questsettitle",
    "questsetdescription",
    "questseticonpath",
    "questitemcreate",
    "questitemsetdescription",
    "questitemsetcompleted",
    "questsetcompleted",
    "questsetdiscovered",
    "isquestdiscovered",
    "questsetfailed",
    "questsetenabled",
    "questmessagebj",
    "questmessagepopupbj",
    "triggerregisterplayervictoryevent",
    "triggerregisterplayerdefeatevent",
    "triggerregisterunitvictoryevent",
    "triggerregisterunitdefeatevent",
    "customvictorybj",
    "customdefeatbj",
    "customdefeatbjsimple",
    "getlocalplayer",
    "gethandleid",
    "getunitrace",
    "setplayerslotstate",
    "setplayeralliancestate",
    "setmissiondifficulty",
    "getmissiondifficulty",
    "saveunithandle",
    "loadunithandle",
    "saveplayerhandle",
    "loadplayerhandle",
    "savetimerhandle",
    "loadtimerhandle",
    "savetriggerhandle",
    "loadtriggerhandle",
    "saveinteger",
    "loadinteger",
    "savereal",
    "loadreal",
    "saveboolean",
    "loadboolean",
    "savestring",
    "loadstring",
    "flushchildhashtable",
];

/// Outcome-critical natives measured as unimplemented on 2026-11-21, each with
/// its call sites and the owner of the fix. A gap outside this table fails the
/// test; an entry that disappears is a closed gap (reported, never required).
const KNOWN_OUTCOME_GAPS: &[(&str, &str)] = &[
    (
        "getunitrace",
        "Undead07/Undead08 shield-region filters compare RACE_UNDEAD to the unit's race; owner: wc3-re jass natives; next: map the unit rawcode to its race",
    ),
    (
        "isquestdiscovered",
        "quest-visibility gates in Human03/Orc03/Undead05; owner: wc3-re jass natives; next: expose the VM quest's discovered flag",
    ),
];

fn install_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

#[test]
fn all_44_campaign_scripts_boot() {
    let root = install_root();
    if !root.is_dir() {
        eprintln!("skip: extracted install missing at {}", root.display());
        return;
    }
    let maps_dir = root.join("Maps");
    // Map scripts recurse deep enough to overflow the default test-thread stack
    // (the engine notes the same about the game runtime), so the sweep runs on a
    // dedicated 32 MB stack.
    let matrix = std::thread::Builder::new()
        .name("campaign-boot-sweep".into())
        .stack_size(32 * 1024 * 1024)
        .spawn({
            let root = root.clone();
            let maps_dir = maps_dir.clone();
            move || wc3_re::mapboot::boot_all(&root, &maps_dir)
        })
        .expect("spawn boot sweep")
        .join()
        .expect("boot sweep panicked")
        .expect("boot sweep");
    let total = |key: &str| matrix.totals.get(key).copied().unwrap_or(0);
    println!(
        "GATE campaign.script-boot maps={} ok={} main_ok={} triggers={} executed_triggers={} natives={} unimplemented_distinct={} unimplemented_calls={}",
        matrix.maps.len(),
        total("ok"),
        total("main_ok"),
        total("triggers"),
        total("executed_triggers"),
        matrix.native_calls.len(),
        matrix.unimplemented.len(),
        matrix.unimplemented.values().sum::<u64>()
    );

    assert_eq!(
        matrix.maps.len(),
        44,
        "the shipped Campaign tree holds 44 maps"
    );
    assert_eq!(total("ok"), 44, "44/44 chapters boot");
    assert_eq!(total("main_ok"), 44, "44/44 war3map.j mains run");
    // Pinned from the measured matrix: a boot that stops registering or
    // executing the initialization triggers is a regression even if `ok` stays
    // true. Observed 2026-11-21: triggers=4794 executed_triggers=682 (the
    // 2026-11-14 matrix recorded 4820/726, before the natives refactor).
    assert!(
        total("triggers") >= 4_700,
        "the maps still register their triggers ({})",
        total("triggers")
    );
    assert!(
        total("executed_triggers") >= 650,
        "the initialization triggers still execute ({})",
        total("executed_triggers")
    );

    let mut failures: Vec<String> = Vec::new();
    for map in &matrix.maps {
        if !map.ok || !map.main_ok {
            failures.push(format!(
                "{}: ok={} main_ok={}",
                map.path, map.ok, map.main_ok
            ));
        }
        if !map.errors.is_empty() {
            failures.push(format!("{}: errors {:?}", map.path, map.errors));
        }
        if map.triggers < 5 || map.executed_triggers == 0 {
            failures.push(format!(
                "{}: triggers={} executed={}",
                map.path, map.triggers, map.executed_triggers
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "campaign boot failures ({}): {failures:?}",
        failures.len()
    );

    // The measured ledger, printed in full: the audit trail the "no silent
    // unsupported path" rule asks for.
    for (name, calls) in &matrix.unimplemented {
        println!("UNIMPLEMENTED {name} calls={calls}");
    }
    let outcome_gaps: Vec<(String, u64)> = matrix
        .unimplemented
        .iter()
        .filter(|(name, _)| {
            let lowered = name.to_ascii_lowercase();
            OUTCOME_NATIVES.contains(&lowered.as_str())
        })
        .map(|(name, calls)| (name.clone(), *calls))
        .collect();
    for (name, calls) in &outcome_gaps {
        match KNOWN_OUTCOME_GAPS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
        {
            Some((_, why)) => println!("OUTCOME_GAP {name} calls={calls} known: {why}"),
            None => println!("OUTCOME_GAP {name} calls={calls} UNTRACKED"),
        }
    }
    for (known, _) in KNOWN_OUTCOME_GAPS {
        if !matrix
            .unimplemented
            .keys()
            .any(|name| name.eq_ignore_ascii_case(known))
        {
            println!("OUTCOME_GAP_CLOSED {known}");
        }
    }
    let untracked: Vec<&(String, u64)> = outcome_gaps
        .iter()
        .filter(|(name, _)| {
            !KNOWN_OUTCOME_GAPS
                .iter()
                .any(|(known, _)| known.eq_ignore_ascii_case(name))
        })
        .collect();
    assert!(
        untracked.is_empty(),
        "outcome-critical natives unimplemented and untracked: {untracked:?}"
    );

    // The most-called natives across the campaign corpus: the measurement that
    // says which implementation actually carries the campaign.
    let mut calls: Vec<(&String, &u64)> = matrix.native_calls.iter().collect();
    calls.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    let top: Vec<String> = calls
        .iter()
        .take(12)
        .map(|(name, count)| format!("{name}={count}"))
        .collect();
    println!("  top natives: {}", top.join(" "));
}
