//! `cinematic.campaign` — a campaign mission's opening runs through the VM and
//! produces real camera/transmission events.
//!
//! The fixture is the retail `Human01.w3m` from the donor install tree: its
//! script is the campaign's first mission, the one with Uther and Arthas in
//! Strahnbrad. The test loads the map's own units and terrain, merges
//! `common.j` + `Blizzard.j` + `war3map.j`, runs the entry points, ticks 300
//! frames, and asserts that:
//!
//! * the intro cinematics actually executed (camera moves / transmissions are
//!   in the host's cinematic log),
//! * no cinematic native the mission uses stayed unresolved.
//!
//! A missing fixture skips with a reason, because the campaign maps are the
//! operator's own game data and are not vendored into the repository.

use std::path::{Path, PathBuf};

use wc3_mpq::MpqArchive;
use wc3_re::mapfiles::MapBundle;
use wc3_sim::jass::host::CinematicEvent;
use wc3_sim::jass::{ScriptBundle, ScriptedSim};
use wc3_sim::Sim;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn human01_path() -> Option<PathBuf> {
    let path = repo_root().join("analysis/war3_full3/Maps/Campaign/Human01.w3m");
    path.is_file().then_some(path)
}

#[test]
fn cinematic_human01_opening_emits_camera_and_transmission_events() {
    let Some(map_path) = human01_path() else {
        eprintln!("skip: analysis/war3_full3/Maps/Campaign/Human01.w3m missing");
        return;
    };
    let Some(scripts_dir) = wc3_sim::jass::find_game_scripts() else {
        eprintln!("skip: common.j/Blizzard.j headers missing");
        return;
    };
    let bundle = MapBundle::open(&map_path).expect("open Human01");
    let script = bundle.script.clone().expect("war3map.j");

    // World geometry and placed units, exactly like the melee integration
    // tests: the script's orders and camera targets refer to these units.
    let archive = MpqArchive::open_at(&map_path, bundle.mpq_base).expect("map archive");
    let terrain = wc3_formats::w3e::Terrain::parse(
        &archive.read("war3map.w3e").expect("war3map.w3e"),
    )
    .expect("terrain");
    let placed = wc3_formats::map::parse_units(
        &archive.read("war3mapUnits.doo").expect("war3mapUnits.doo"),
    )
    .expect("units");

    let mut sim = Sim::new(0x0C1E, terrain.width, terrain.height);
    sim.passive_income = false;
    for unit in &placed {
        let type_id = String::from_utf8_lossy(&unit.type_id).to_string();
        let _ = sim.spawn_def_exact(
            unit.player.min(23) as u8,
            &type_id,
            wc3_sim::Vec2::new(unit.position.x, unit.position.y),
        );
    }

    let mut script_bundle = ScriptBundle::new();
    script_bundle
        .add_game_scripts(&scripts_dir)
        .expect("common.j + Blizzard.j");
    script_bundle
        .add_source("war3map.j", &script)
        .expect("Human01 script parses");

    let mut scripted = ScriptedSim::new(sim, &script_bundle, 0x0C1E).expect("compile Human01");
    let report = scripted.run_map_entry_points();
    assert!(
        report.steps > 1_000 && report.triggers_registered > 0,
        "Human01 entry points must run and register triggers: {report:?}"
    );

    // 300 ticks = 10 s at the engine's 30 Hz.
    for _ in 0..300 {
        scripted.step(&[]);
    }

    let vm = scripted.vm();
    // Diagnostics: what actually ran, and what trapped.
    eprintln!(
        "entry: steps={} triggers={} natives={} units_created={} traps={:?}",
        report.steps, report.triggers_registered, report.natives_called, report.units_created,
        report.traps.iter().take(3).collect::<Vec<_>>()
    );
    eprintln!(
        "errors={} unresolved_distinct={} native_calls={}",
        vm.errors.len(),
        vm.unresolved_natives().len(),
        vm.native_calls.len()
    );
    for error in vm.errors.iter().take(5) {
        eprintln!("  error: {error}");
    }
    let cinematic_functions: Vec<(&String, &u64)> = vm
        .function_calls
        .iter()
        .filter(|(name, _)| name.to_ascii_lowercase().contains("cinematic"))
        .collect();
    eprintln!("cinematic functions called: {cinematic_functions:?}");
    let init_functions: Vec<(&String, &u64)> = vm
        .function_calls
        .iter()
        .filter(|(name, _)| {
            let lowered = name.to_ascii_lowercase();
            lowered.contains("map_init") || lowered.contains("setupquest") || lowered.contains("intro_cinematic_actions")
        })
        .collect();
    eprintln!("init functions called: {init_functions:?}");
    eprintln!("threads={}", vm.threads.len());
    let events: Vec<&CinematicEvent> = vm.host.cutscene_events().collect();
    let cameras = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                CinematicEvent::CameraApply { .. }
                    | CinematicEvent::CameraPosition { .. }
                    | CinematicEvent::CameraController { .. }
            )
        })
        .count();
    let transmissions = events
        .iter()
        .filter(|event| matches!(event, CinematicEvent::Transmission { .. }))
        .count();
    eprintln!(
        "human01: events={} cameras={cameras} transmissions={transmissions} cinematic_mode={}",
        events.len(),
        vm.host.cinematic_mode()
    );
    for event in events.iter().take(8) {
        eprintln!("  {}", event.describe());
    }
    assert!(
        cameras + transmissions >= 1,
        "the opening must produce at least one camera/transmission beat; got {} events: {:?}",
        events.len(),
        events.iter().map(|event| event.describe()).collect::<Vec<_>>()
    );

    // The cinematic natives the mission uses must be handled, not merely
    // resolved to a typed default. `unresolved` is the VM's own record of
    // calls that reached no handler.
    let critical = [
        "setcinematicscene",
        "setcinematicscene",
        "camerasetupapplyforce duration",
        "camerasetupapplyforceduration",
        "camerasetupapply",
        "camerasetupgetfield",
        "cameratargetpositionx",
        "getcameratargetpositionx",
        "getcameratargetpositiony",
        "getcameratargetpositionloc",
        "isquestcompleted",
        "isquestitemcompleted",
        "triggerregistertimerevent",
    ];
    eprintln!(
        "unresolved: {:?}",
        vm.unresolved_natives()
            .iter()
            .map(|(name, count)| format!("{name}x{count}"))
            .collect::<Vec<_>>()
    );
    let unresolved: Vec<(&String, &u64)> = vm
        .unresolved_natives()
        .iter()
        .filter(|(name, _)| {
            let lowered = name.to_ascii_lowercase();
            critical.iter().any(|needle| lowered.contains(needle))
        })
        .collect();
    assert!(
        unresolved.is_empty(),
        "campaign cinematic natives unresolved: {unresolved:?}"
    );
}
