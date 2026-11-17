//! Round-2 game-runtime wiring: map script → InitCustomTriggers, host/Storm
//! boot, AI scripts, and object-data/text overrides.
//!
//! Fixtures are the operator's local extracted RoC install:
//! `analysis/war3_full3/Maps/Campaign/Human04.w3m`, `Scripts/common.ai`,
//! `Scripts/human.ai`, `Units/*.slk|txt`.

use std::path::{Path, PathBuf};

use wc3_re::engine::{Engine, EngineConfig};
use wc3_re::host::HostServices;
use wc3_re::mapfiles::MapBundle;
use wc3_re::objectdata::ObjectDb;

fn install() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

#[test]
fn host_launch_mounts_storm_and_runs_map_script() {
    let root = install();
    let map = root.join("Maps/Campaign/Human04.w3m");
    if !map.is_file() {
        eprintln!("skip: extracted install missing");
        return;
    }
    let mut config = EngineConfig::new(&root);
    config.map = Some(map);
    let mut host = HostServices::default();
    let engine = host.launch(config).expect("host launch");
    assert_eq!(host.calls.len(), 3, "A372/A327/A475 host calls");
    assert_eq!(host.calls[0].ordinal, 372);
    assert_eq!(host.calls[1].ordinal, 327);
    assert_eq!(host.calls[2].ordinal, 475);
    assert!(!engine.storm.listfile().is_empty(), "Storm loose tree");
    assert!(
        !engine
            .storm
            .read("Units\\UnitData.slk")
            .expect("UnitData through StormFs")
            .is_empty()
    );
    let report = engine.script_report();
    assert!(report.main_ok, "map main: {:?}", engine.script_errors);
    assert!(report.triggers > 0, "InitCustomTriggers registered no triggers");
    assert!(report.executed_triggers > 0, "map init trigger did not execute");
    let spawn = engine.map_spawn();
    assert_eq!(spawn.map_units, 90, "Human04 war3mapUnits.doo placement count");
    assert_eq!(
        spawn.spawned_units + spawn.matched_units,
        spawn.map_units,
        "every map unit is spawned or matched to a script unit"
    );
    assert!(spawn.map_doodads > 0, "war3map.doo has no placements");
    assert_eq!(
        spawn.spawned_doodads + spawn.matched_doodads,
        spawn.map_doodads,
        "every doodad is spawned or matched to a script destructable"
    );
    assert!(
        engine
            .runtime()
            .is_some_and(|runtime| !runtime.world.destructables.is_empty()),
        "JASS world has destructables after map boot"
    );
    // Every `war3mapUnits.doo` placement has a live JASS unit at its type and
    // position. The world can hold extra script-only units (Human04's
    // CreateAllUnits covers 83 of 90 and creates 2 cinematics units), so the
    // exact in-game count is `map_units + script_only`, not `map_units`.
    let runtime = engine.runtime().expect("runtime");
    let map = engine.map().expect("map bundle");
    let placements = map.units.as_ref().expect("war3mapUnits.doo placements");
    for placed in placements {
        let id = wc3_re::fourcc_str(&placed.type_id);
        assert!(
            runtime.world.units.values().any(|unit| {
                unit.id.eq_ignore_ascii_case(&id)
                    && (unit.x - f64::from(placed.position.x)).abs() < 4.0
                    && (unit.y - f64::from(placed.position.y)).abs() < 4.0
            }),
            "placement {id} at ({:.0},{:.0}) missing from JASS world",
            placed.position.x,
            placed.position.y
        );
    }
    assert!(
        runtime.world.units.len() >= spawn.map_units,
        "world holds every placement plus script-only units: {} >= {}",
        runtime.world.units.len(),
        spawn.map_units
    );
}

#[test]
fn ai_scripts_load_and_run_init() {
    let root = install();
    let scripts = root.join("Scripts");
    if !scripts.is_dir() {
        eprintln!("skip: Scripts missing");
        return;
    }
    let mut engine = Engine::boot(EngineConfig::new(&root)).expect("engine boot");
    let loaded = engine.load_ai_scripts(&scripts).expect("load AI scripts");
    assert!(loaded > 90, "AI scripts {loaded}");
    let common = std::fs::read_to_string(scripts.join("common.ai")).expect("common.ai");
    let common_program = wc3_re::jass::parse(&common).expect("parse common.ai");
    let human = engine
        .ai_scripts()
        .iter()
        .find(|script| script.path.ends_with("human.ai"))
        .expect("human.ai");
    let run = human.run_init(&common_program, 1).expect("InitAI");
    assert!(run.ok, "InitAI: {:?}", run.error);
}

#[test]
fn object_data_and_wts_overrides_reach_object_db() {
    let root = install();
    let map_path = root.join("Maps/Campaign/Human04.w3m");
    if !map_path.is_file() {
        eprintln!("skip: extracted install missing");
        return;
    }
    let mut db = ObjectDb::load_dir(&root).expect("object db");
    assert!(!db.metas().is_empty(), "metadata tables");
    assert!(!db.texts().is_empty(), "profile text layers");
    let bundle = MapBundle::open(&map_path).expect("map bundle");
    let mut applied = 0;
    for objects in bundle.objects.values() {
        applied += db.apply_map_objects(objects);
    }
    assert!(applied > 0, "map object overrides applied");
    if let Some(wts) = &bundle.wts {
        let _ = db.resolve_texts(wts);
    }
    assert!(db.record("hfoo").is_some(), "base footman survives overlay");
}
