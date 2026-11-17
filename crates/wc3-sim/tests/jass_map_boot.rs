//! `jass.map-boot` — `(2)Justice.w3x` boots through the VM: map units are loaded
//! from the archive, the script runs `config`+`main`, and the combined world
//! ticks with the script's timers and triggers alive.
//!
//! This is the integration proof for the lane: the script is executed against the
//! same `Sim` the rest of the engine uses, not a side world. The fixture is the
//! donor copy of the Blizzard melee map, so the test skips on a donor-only
//! checkout exactly like `justice_1v1.rs` does.

mod common;

use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_formats::w3e::Terrain;
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::jass::{ScriptBundle, ScriptedSim};
use wc3_sim::{Sim, Vec2};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn justice_path() -> Option<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    path.is_file().then_some(path)
}

fn extracted_script() -> Option<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-jass/extracted");
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "j")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("Justice"))
        })
        .collect();
    entries.sort();
    entries.into_iter().next()
}

#[test]
fn justice_boots_through_the_script_vm_and_keeps_ticking() {
    let Some(map_path) = justice_path() else {
        eprintln!("skip: (2)Justice.w3x fixture missing");
        return;
    };
    let Some(script_path) = extracted_script() else {
        eprintln!("skip: extracted Justice war3map.j missing (run analysis/agent-jass/extract_maps.sh)");
        return;
    };
    let Some(scripts_dir) = wc3_sim::jass::find_game_scripts() else {
        eprintln!("skip: game headers missing");
        return;
    };

    // World from the map's own data, exactly like the melee integration tests.
    let base = map_mpq_base(&map_path).expect("mpq base");
    let archive = MpqArchive::open_at(&map_path, base).expect("open map");
    let terrain = Terrain::parse(&archive.read("war3map.w3e").expect("w3e")).expect("terrain");
    let pathing = PathingMap::parse(&archive.read("war3map.wpm").expect("wpm")).expect("wpm");
    let placed = parse_units(&archive.read("war3mapUnits.doo").expect("units")).expect("units");
    // The map's own data tables: `war3map.wts` backs `GetLocalizedString`,
    // `war3map.w3i` backs `IsMapFlagSet`. Both live in the archive, so the boot
    // path proves the natives read the map instead of an empty table.
    let wts = archive.read("war3map.wts").ok();
    let w3i = archive.read("war3map.w3i").ok();

    let mut sim = Sim::new(0x1EAF, terrain.width, terrain.height);
    let _ = pathing;
    sim.passive_income = false;
    for unit in &placed {
        let id = match unit.player {
            0 => 0u8,
            1 => 1u8,
            // Neutral units only exist in the data roster for a subset of ids;
            // `spawn_def_exact` returning `None` is the honest outcome and the
            // script-created neutral pass reports the same gap.
            other => (other.min(23)) as u8,
        };
        let type_id = String::from_utf8_lossy(&unit.type_id).to_string();
        let _ = sim.spawn_def_exact(id, &type_id, Vec2::new(unit.position.x, unit.position.y));
    }

    let mut bundle = ScriptBundle::new();
    bundle.add_game_scripts(&scripts_dir).expect("headers");
    bundle.add_map_script(&script_path).expect("map script");
    if let Some(wts) = &wts {
        let entries = bundle.set_wts(wts);
        eprintln!("justice: war3map.wts entries={entries}");
    }
    if let Some(w3i) = &w3i
        && bundle.add_map_w3i(w3i)
    {
        let word = bundle.map_flags.unwrap_or(0);
        eprintln!(
            "justice: war3map.w3i flags={word:#06x} melee={} hide_minimap={}",
            word & (1 << 2) != 0,
            word & 1 != 0
        );
        assert!(
            word & (1 << 2) != 0,
            "a melee map must set MeleeMap in its header: {word:#06x}"
        );
    }
    let mut scripted = ScriptedSim::new(sim, &bundle, 0x1EAF).expect("compile");
    let report = scripted.run_map_entry_points();
    assert_eq!(
        scripted.vm().host.map_flags,
        bundle.map_flags,
        "the VM carries the map's flags word"
    );
    if wts.is_some() {
        assert!(
            scripted.vm().host.strings.map_len() > 0,
            "the VM carries the map's trigger strings"
        );
    }

    eprintln!(
        "justice: steps={} units_created={} triggers={} native_calls={} units={}",
        report.steps,
        report.units_created,
        report.triggers_registered,
        report.natives_called,
        scripted.sim().units.len()
    );
    assert!(report.steps > 10_000, "config/main must execute: {report:?}");
    assert!(
        report.triggers_registered >= 20,
        "triggers registered: {}",
        report.triggers_registered
    );
    assert_eq!(
        scripted.unresolved_natives().len(),
        0,
        "{:?}",
        scripted.unresolved_natives()
    );

    // Script time and simulation ticks advance together for 10 s.
    let start_units = scripted.sim().units.len();
    for _ in 0..300 {
        scripted.step(&[]);
    }
    assert_eq!(scripted.vm().tick, 300, "script clock follows the sim tick");
    assert!(
        (scripted.script_time() - 10.0).abs() < 0.01,
        "script time {}",
        scripted.script_time()
    );
    assert!(
        scripted.sim().tick >= 300,
        "the simulation kept stepping: {}",
        scripted.sim().tick
    );
    assert_eq!(
        scripted.unresolved_natives().len(),
        0,
        "no native may appear after the first tick either: {:?}",
        scripted.unresolved_natives()
    );
    // The map still has its units: the script did not wipe the world.
    assert!(
        scripted.sim().units.len() >= start_units.min(1),
        "units after 10 s: {}",
        scripted.sim().units.len()
    );
    assert!(
        scripted.vm().native_calls.len() >= 30,
        "natives called: {:?}",
        scripted.vm().native_calls.len()
    );
}
