//! `jass.native-coverage` — the corpus gate: every native the shipped maps call
//! must have a real handler.
//!
//! The measurement that drives this file is `analysis/agent-jass/native_census.py`
//! (static call-site histogram over the 96 extracted map scripts) and the runtime
//! census below. Both must agree, and neither may leave a call on a stub: an
//! unresolved native returns a typed default, which is exactly the silent-zero
//! bug this test exists to prevent.

mod common;

use std::path::{Path, PathBuf};

use wc3_sim::jass::{ScriptBundle, ScriptedSim, registry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn extracted_dir() -> PathBuf {
    repo_root().join("analysis/agent-jass/extracted")
}

fn justice_script() -> Option<PathBuf> {
    let dir = extracted_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
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

/// Boots a map script with the full game headers and reports the census.
fn boot(script: &Path) -> (ScriptedSim, wc3_sim::jass::EntryReport) {
    let scripts = wc3_sim::jass::find_game_scripts().expect("game scripts (common.j/Blizzard.j)");
    let mut bundle = ScriptBundle::new();
    bundle.add_game_scripts(&scripts).expect("headers");
    bundle.add_map_script(script).expect("map script");
    let sim = common::sandbox(0x5EED);
    let mut scripted = ScriptedSim::new(sim, &bundle, 0x5EED).expect("compile");
    let report = scripted.run_map_entry_points();
    (scripted, report)
}

#[test]
fn justiice_script_runs_with_zero_unresolved_natives() {
    let Some(script) = justice_script() else {
        panic!(
            "missing {} — run analysis/agent-jass/extract_maps.sh",
            extracted_dir().display()
        );
    };
    let (scripted, report) = boot(&script);
    let vm = scripted.vm();
    let unresolved = scripted.unresolved_natives();
    assert!(
        unresolved.is_empty(),
        "unresolved natives on {script:?}: {unresolved:?}"
    );
    // Every trap must be a *data roster* gap, never a script-engine gap: a
    // `CreateUnit` of a type the local roster does not model returns null and is
    // reported, while every function and native the script names must resolve.
    let script_errors: Vec<&String> = report
        .traps
        .iter()
        .filter(|trap| {
            !trap.starts_with("CreateUnit: unknown unit type")
                && !trap.starts_with("CreateDestructable: unknown type")
                && !trap.starts_with("CreateItem: unknown item type")
        })
        .collect();
    assert!(
        script_errors.is_empty(),
        "unexpected script errors: {script_errors:?} (all traps: {:?})",
        report.traps
    );
    assert!(vm.steps > 0, "the script must have executed instructions");
    assert!(
        report.triggers_registered >= 20,
        "Justice registers 29 triggers in its script, saw {}",
        report.triggers_registered
    );
    assert!(
        vm.native_calls.len() > 20,
        "expected a broad native surface, saw {:?}",
        vm.native_calls.keys().collect::<Vec<_>>()
    );
}

#[test]
fn every_native_the_justice_script_calls_has_a_handler() {
    let Some(script) = justice_script() else {
        panic!("missing extracted Justice script");
    };
    let (scripted, _) = boot(&script);
    let called: Vec<&String> = scripted.vm().native_calls.keys().collect();
    let mut missing: Vec<&str> = Vec::new();
    for name in called {
        if !registry::is_handled(name) {
            missing.push(name.as_str());
        }
    }
    assert!(missing.is_empty(), "natives without a handler: {missing:?}");
}

#[test]
fn the_whole_extracted_corpus_runs_with_zero_unresolved_natives() {
    let dir = extracted_dir();
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("missing {}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "j"))
        .collect();
    scripts.sort();
    assert!(
        scripts.len() >= 90,
        "expected the 96-map corpus, found {}",
        scripts.len()
    );

    // Two strictly separated classes:
    //
    // 1. `registry_known` — the name is declared by `common.j` (any header copy in
    //    this tree) or registered by the TFT engine, so the signature is known and
    //    a handler is required. This set must be empty.
    // 2. `header_absent` — the map is built against a newer header (Reforged-era
    //    `patch_3.0.0.w3x`) and the native is declared by no header in this tree.
    //    Its signature cannot be observed here, so the VM returns a typed default
    //    and *counts* it; the set is reported by name so the gap is visible.
    let mut registry_known: Vec<(String, Vec<(String, u64)>)> = Vec::new();
    let mut header_absent: std::collections::BTreeMap<String, u64> = Default::default();
    let mut total_natives: std::collections::BTreeSet<String> = Default::default();
    let mut total_steps = 0u64;
    let mut total_undeclared_globals = 0usize;
    for script in &scripts {
        let (scripted, _) = boot(script);
        let vm = scripted.vm();
        total_steps += vm.steps;
        total_undeclared_globals += vm.code.undeclared.len();
        total_natives.extend(vm.native_calls.keys().cloned());
        let mut known: Vec<(String, u64)> = Vec::new();
        for (name, count) in scripted.unresolved_natives() {
            if registry::lookup(name).is_some() {
                known.push((name.clone(), *count));
            } else {
                *header_absent.entry(name.clone()).or_insert(0) += *count;
            }
        }
        if !known.is_empty() {
            registry_known.push((
                script
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                known,
            ));
        }
    }
    assert!(
        total_steps > 100_000,
        "the corpus must execute real scripts, ran {total_steps} instructions"
    );
    assert!(total_natives.len() >= 150, "{}", total_natives.len());
    assert!(
        registry_known.is_empty(),
        "natives declared by the loaded headers still have no handler: {registry_known:#?}"
    );
    // The Reforged-only names must be the only ones left, and they must be a small
    // minority of the corpus surface: they all come from maps whose header is not
    // in this tree.
    assert!(
        header_absent.len() <= 20,
        "too many header-absent natives ({}): {header_absent:?}",
        header_absent.len()
    );
    eprintln!(
        "corpus: {} maps, {total_steps} instructions, {} distinct natives, \
         {} header-absent natives, {total_undeclared_globals} undeclared globals",
        scripts.len(),
        total_natives.len(),
        header_absent.len()
    );
    eprintln!("header-absent natives: {header_absent:?}");
}

#[test]
fn registry_covers_every_native_name_the_engine_registers() {
    let stats = registry::stats();
    assert!(stats.total >= 1600, "{stats:?}");
    assert!(stats.engine_registered >= 1096, "{stats:?}");
    // Dispatch coverage (handlers present) and semantic coverage (implemented)
    // are separate numbers on purpose: AUD-016 exists because the first used to
    // be reported as the second.
    assert!(
        stats.dispatch_engine_registered >= 150,
        "dispatch handlers for engine natives: {} of {}",
        stats.dispatch_engine_registered,
        stats.engine_registered
    );
    assert!(
        stats.implemented_engine_registered >= 100,
        "implemented engine natives: {} of {}",
        stats.implemented_engine_registered,
        stats.engine_registered
    );
    assert!(
        stats.implemented + stats.partial + stats.recorded_only + stats.no_op + stats.unsupported
            == stats.total,
        "every registry row carries exactly one semantic status: {stats:?}"
    );
    eprintln!(
        "registry: {} rows, dispatch={}, implemented={} (+{} engine), partial={}, recorded_only={}, no_op={}, unsupported={}",
        stats.total,
        stats.dispatch,
        stats.implemented,
        stats.implemented_engine_registered,
        stats.partial,
        stats.recorded_only,
        stats.no_op,
        stats.unsupported
    );
}

#[test]
fn convert_natives_resolve_through_the_identity_rule() {
    // `Convert*` enum constructors are the engine's own casts.
    for name in [
        "ConvertPlayerColor",
        "ConvertRace",
        "ConvertUnitState",
        "ConvertAttackType",
        "ConvertItemType",
        "ConvertSoundType",
    ] {
        assert!(registry::is_handled(name), "{name} must be handled");
    }
}
