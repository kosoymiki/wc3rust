//! Registry `docs/audit/WC3RUST_CODE_AUDIT.md` items AUD-012, AUD-013,
//! AUD-017, AUD-021 and AUD-022 — one regression test per item, named after
//! the registry id.
//!
//! Every test here was written before its patch and run against the unpatched
//! tree; the failing run is the reproduction evidence recorded in
//! `docs/audit/status-sim2.md`. The tests use only the public API, so the same
//! file compiled and failed on the old code and passes on the fixed code.

mod common;

use common::sandbox;
use wc3_sim::Vec2;
use wc3_sim::jass::vm::Instr;
use wc3_sim::jass::{ScriptBundle, Value};
use wc3_sim::{Order, Sim};

/// Attaches `source` to a fresh sandbox, or `None` when `with_headers` is asked
/// for and the game scripts are not installed (the `common.j`/`Blizzard.j`
/// copies live under the untracked `analysis/` and `donors/` trees, so a clean
/// clone has none — the rest of the JASS-native suite skips the same way).
///
/// `with_headers` matters for what the compiler resolves: with the headers a
/// declared native compiles to `Instr::Native`; without them the same name is
/// reached through the runtime name dispatch (`Instr::Unknown`), which is the
/// indirect path AUD-017 is about.
fn attach(seed: u64, source: &str, with_headers: bool) -> Option<Sim> {
    let mut sim = sandbox(seed);
    let mut bundle = ScriptBundle::new();
    if with_headers {
        let dir = wc3_sim::jass::find_game_scripts()?;
        bundle.add_game_scripts(&dir).expect("headers");
    }
    bundle.add_source("audit_sim2.j", source).expect("parse");
    sim.attach_script(&bundle, seed).expect("compile");
    Some(sim)
}

fn run(sim: &mut Sim, name: &str) -> Value {
    sim.with_script(|vm| vm.run_function(name))
        .expect("script attached")
        .expect("function runs")
}

fn global_int(sim: &Sim, name: &str) -> i64 {
    let vm = sim.script().expect("script attached");
    let index = vm.code.global_by_name[name] as usize;
    vm.globals[index].int() as i64
}

fn global_bool(sim: &Sim, name: &str) -> bool {
    let vm = sim.script().expect("script attached");
    let index = vm.code.global_by_name[name] as usize;
    vm.globals[index].truthy()
}

fn global_real(sim: &Sim, name: &str) -> f32 {
    let vm = sim.script().expect("script attached");
    let index = vm.code.global_by_name[name] as usize;
    vm.globals[index].real()
}

/// AUD-012: an invalid, destroyed, or wrong-family `location` handle must be a
/// recorded diagnostic with no simulation side effect — never a spawn or a move
/// at the world origin.
#[test]
fn aud012_invalid_location_handles_are_diagnostics_not_the_world_origin() {
    let Some(mut sim) = attach(
        0xA0D0_0120,
        r#"
        globals
            unit mover = null
            integer created = 0
        endglobals
        function create_at_null takes nothing returns nothing
            call CreateUnitAtLoc(Player(0), 'hfoo', null, 270.0)
        endfunction
        function create_at_destroyed takes nothing returns nothing
            local location gone = Location(1000.0, 1000.0)
            call RemoveLocation(gone)
            call CreateUnitAtLoc(Player(0), 'hfoo', gone, 270.0)
        endfunction
        function create_at_wrong_kind takes nothing returns nothing
            call CreateUnitAtLoc(Player(0), 'hfoo', CreateGroup(), 270.0)
        endfunction
        function create_by_name_at_null takes nothing returns nothing
            call CreateUnitAtLocByName(Player(0), "hfoo", null, 270.0)
        endfunction
        function spawn_mover takes nothing returns nothing
            set mover = CreateUnit(Player(0), 'hfoo', 1500.0, 1500.0, 0.0)
        endfunction
        function move_to_null takes nothing returns nothing
            call SetUnitPositionLoc(mover, null)
        endfunction
        function move_to_destroyed takes nothing returns nothing
            local location gone = Location(200.0, 200.0)
            call RemoveLocation(gone)
            call SetUnitPositionLoc(mover, gone)
        endfunction
        function move_to_wrong_kind takes nothing returns nothing
            call SetUnitPositionLoc(mover, CreateGroup())
        endfunction
        function order_to_null takes nothing returns nothing
            call IssuePointOrderLoc(mover, "move", null)
        endfunction
        function order_by_id_to_null takes nothing returns nothing
            call IssuePointOrderByIdLoc(mover, 'move', null)
        endfunction
        "#,
        true,
    ) else {
        eprintln!("skip: game headers (common.j/Blizzard.j) are not installed");
        return;
    };

    let units_before = sim.units.len();
    for function in [
        "create_at_null",
        "create_at_destroyed",
        "create_at_wrong_kind",
        "create_by_name_at_null",
    ] {
        run(&mut sim, function);
    }
    assert_eq!(
        sim.units.len(),
        units_before,
        "no unit may be created from an invalid location handle"
    );
    assert!(
        sim.units
            .iter()
            .all(|unit| unit.pos != Vec2::ZERO),
        "no unit may appear at the world origin"
    );
    let errors = sim.script().expect("script").errors.clone();
    assert!(
        errors.iter().any(|error| error.contains("CreateUnitAtLoc")),
        "the invalid location is a diagnostic: {errors:?}"
    );

    run(&mut sim, "spawn_mover");
    let mover = sim
        .units_of(0)
        .find(|unit| unit.def().id == "hfoo" && unit.pos == Vec2::new(1500.0, 1500.0))
        .map(|unit| unit.id)
        .expect("mover spawned at its scripted position");
    let before = sim.units.len();
    for function in ["move_to_null", "move_to_destroyed", "move_to_wrong_kind"] {
        run(&mut sim, function);
        assert_eq!(
            sim.unit(mover).map(|unit| unit.pos),
            Some(Vec2::new(1500.0, 1500.0)),
            "{function}: an invalid location must not move the unit"
        );
    }
    assert_eq!(sim.units.len(), before, "no spawn from a move native");

    for function in ["order_to_null", "order_by_id_to_null"] {
        run(&mut sim, function);
        assert!(
            matches!(sim.unit(mover).map(|unit| unit.order), Some(Order::Idle)),
            "{function}: an invalid location must not issue a move to the origin"
        );
    }
    let errors = sim.script().expect("script").errors.clone();
    assert!(
        errors.iter().any(|error| error.contains("SetUnitPositionLoc")),
        "the move diagnostic is recorded: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("IssuePointOrderLoc")),
        "the order diagnostic is recorded: {errors:?}"
    );
}

/// AUD-013: `UnitAddType` / `UnitRemoveType` change the instance's runtime
/// classification, a second instance of the same def is unaffected, the calls
/// are idempotent, the classification feeds `IsUnitType` *and* the structure
/// count filter, and it survives a save/load round trip.
#[test]
fn aud013_unit_add_and_remove_type_change_the_runtime_classification() {
    let Some(mut sim) = attach(
        0xA0D0_0130,
        r#"
        globals
            unit u = null
            unit v = null
            boolean u_hero_before = true
            boolean u_hero_after = false
            boolean v_hero_after = true
            boolean u_hero_again = false
            boolean u_ranged_before = true
            boolean u_ranged_after = false
            boolean u_ranged_removed = true
            boolean u_structure_after_remove = true
            boolean v_structure = false
            boolean u_structure = true
            boolean v_ranged = true
            integer structures_before = -1
            integer structures_added = -1
            integer structures_removed = -1
            boolean add_returns = false
            boolean remove_returns = false
            boolean bad_type_returns = true
            boolean restored_u_hero = false
            boolean restored_v_hero = true
            boolean restored_u_ranged = true
        endglobals
        function setup takes nothing returns nothing
            set u = CreateUnit(Player(0), 'hfoo', 1000.0, 1000.0, 0.0)
            set v = CreateUnit(Player(0), 'hfoo', 1300.0, 1000.0, 0.0)
            set u_hero_before = IsUnitType(u, UNIT_TYPE_HERO)
            set add_returns = UnitAddType(u, UNIT_TYPE_HERO)
            set u_hero_after = IsUnitType(u, UNIT_TYPE_HERO)
            set v_hero_after = IsUnitType(v, UNIT_TYPE_HERO)
            set u_hero_again = UnitAddType(u, UNIT_TYPE_HERO)
            set u_ranged_before = IsUnitType(u, UNIT_TYPE_RANGED_ATTACKER)
            set v_ranged = IsUnitType(v, UNIT_TYPE_RANGED_ATTACKER)
            call UnitAddType(u, UNIT_TYPE_RANGED_ATTACKER)
            set u_ranged_after = IsUnitType(u, UNIT_TYPE_RANGED_ATTACKER)
            set remove_returns = UnitRemoveType(u, UNIT_TYPE_RANGED_ATTACKER)
            set u_ranged_removed = IsUnitType(u, UNIT_TYPE_RANGED_ATTACKER)
            call UnitRemoveType(u, UNIT_TYPE_STRUCTURE)
            set u_structure_after_remove = IsUnitType(u, UNIT_TYPE_STRUCTURE)
            set structures_before = GetPlayerStructureCount(Player(0))
            call UnitAddType(v, UNIT_TYPE_STRUCTURE)
            set v_structure = IsUnitType(v, UNIT_TYPE_STRUCTURE)
            set u_structure = IsUnitType(u, UNIT_TYPE_STRUCTURE)
            set structures_added = GetPlayerStructureCount(Player(0))
            call UnitRemoveType(v, UNIT_TYPE_STRUCTURE)
            set structures_removed = GetPlayerStructureCount(Player(0))
            set bad_type_returns = UnitAddType(u, 200)
        endfunction
        function probe_after_load takes nothing returns nothing
            set restored_u_hero = IsUnitType(u, UNIT_TYPE_HERO)
            set restored_v_hero = IsUnitType(v, UNIT_TYPE_HERO)
            set restored_u_ranged = IsUnitType(u, UNIT_TYPE_RANGED_ATTACKER)
        endfunction
        "#,
        true,
    ) else {
        eprintln!("skip: game headers (common.j/Blizzard.j) are not installed");
        return;
    };
    run(&mut sim, "setup");

    assert!(!global_bool(&sim, "u_hero_before"), "a footman is not a hero");
    assert!(global_bool(&sim, "add_returns"), "UnitAddType reports success");
    assert!(
        global_bool(&sim, "u_hero_after"),
        "UnitAddType must change IsUnitType on the same instance"
    );
    assert!(
        !global_bool(&sim, "v_hero_after"),
        "one instance must not change another instance of the same UnitDef"
    );
    assert!(global_bool(&sim, "u_hero_again"), "a second add is idempotent");
    assert!(!global_bool(&sim, "u_ranged_before"), "a footman is melee");
    assert!(!global_bool(&sim, "v_ranged"), "a footman is melee");
    assert!(
        global_bool(&sim, "u_ranged_after"),
        "UNIT_TYPE_RANGED_ATTACKER (8, common.j) must be addable"
    );
    assert!(global_bool(&sim, "remove_returns"), "UnitRemoveType reports success");
    assert!(
        !global_bool(&sim, "u_ranged_removed"),
        "UnitRemoveType must clear the added bit"
    );
    assert!(
        !global_bool(&sim, "u_structure_after_remove"),
        "removing a type the unit never had stays false"
    );

    // `GetPlayerStructureCount` asks the same classification question, so the
    // sandbox hall counts, the scripted add counts, and the remove stops
    // counting.
    assert_eq!(global_int(&sim, "structures_before"), 1, "the sandbox hall");
    assert!(global_bool(&sim, "v_structure"), "the added structure type is visible");
    assert!(!global_bool(&sim, "u_structure"), "u never became a structure");
    assert_eq!(global_int(&sim, "structures_added"), 2, "added structure counts");
    assert_eq!(global_int(&sim, "structures_removed"), 1, "removed structure stops counting");
    assert!(
        !global_bool(&sim, "bad_type_returns"),
        "an out-of-range unittype is refused, not silently applied"
    );
    let errors = sim.script().expect("script").errors.clone();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("UnitAddType") && error.contains("200")),
        "the out-of-range type is a diagnostic: {errors:?}"
    );

    // The runtime metadata is part of the script world: a save keeps it.
    let bytes = sim.to_save_bytes();
    let mut restored = Sim::from_save(&bytes).expect("save loads");
    restored
        .with_script(|vm| vm.run_function("probe_after_load"))
        .expect("script attached")
        .expect("probe runs");
    assert!(
        global_bool(&restored, "restored_u_hero"),
        "the hero bit assigned by UnitAddType survives the save"
    );
    assert!(
        !global_bool(&restored, "restored_v_hero"),
        "the other instance is still untyped after the load"
    );
    assert!(
        !global_bool(&restored, "restored_u_ranged"),
        "the removed ranged bit stays removed after the load"
    );
    assert_eq!(restored.state_hash(), sim.state_hash());
}

/// AUD-017: `TriggerSleepAction` reached through the runtime name dispatch — the
/// path the compiler cannot see — parks the calling thread, resumes it on the
/// tick grid, and returns the elapsed seconds. The `Wait` instruction a direct
/// call compiles to is asserted alongside it so the shared parking code cannot
/// regress.
#[test]
fn aud017_indirect_trigger_sleep_action_parks_the_thread() {
    // (1) The ordinary JASS call compiles to `Instr::Wait`.
    let Some(mut ordinary) = attach(
        0xA0D0_0171,
        r#"
        globals
            integer phase = 0
            real slept = 0.0
        endglobals
        function delayed takes nothing returns nothing
            set phase = 1
            set slept = TriggerSleepAction(0.4)
            set phase = 2
        endfunction
        "#,
        true,
    ) else {
        eprintln!("skip: game headers (common.j/Blizzard.j) are not installed");
        return;
    };
    run(&mut ordinary, "delayed");
    assert_eq!(
        global_int(&ordinary, "phase"),
        1,
        "the `Wait` instruction still parks the thread"
    );
    for _ in 0..11 {
        ordinary.step(&[]);
    }
    assert_eq!(global_int(&ordinary, "phase"), 1, "still parked before 12 ticks");
    ordinary.step(&[]);
    assert_eq!(global_int(&ordinary, "phase"), 2, "the `Wait` path still resumes");
    assert!(
        global_real(&ordinary, "slept") >= 0.39,
        "the `Wait` path returns the elapsed time: {}",
        global_real(&ordinary, "slept")
    );

    // (2) The indirect path: a name the compiler cannot resolve statically is
    // emitted as `Instr::Unknown` and dispatched by name at run time. The
    // instruction stream below is exactly that shape (the JASS-level spelling of
    // a name the compiler knows would be lowered to `Wait`, so the dispatch path
    // is exercised at the executor level, where the handler really runs).
    let mut indirect = attach(
        0xA0D0_0172,
        r#"
        globals
            integer phase = 0
            real half_slept = -1.0
            real negative_slept = -1.0
            real zero_slept = -1.0
        endglobals
        function staged takes nothing returns nothing
            set phase = 0
        endfunction
        "#,
        false,
    )
    .expect("headerless bundle compiles");
    let (func, phase_global, half_global, negative_global, zero_global) = {
        let code = &indirect.script().expect("script").code;
        (
            code.by_name["staged"],
            code.global_by_name["phase"],
            code.global_by_name["half_slept"],
            code.global_by_name["negative_slept"],
            code.global_by_name["zero_slept"],
        )
    };
    indirect
        .with_script(|vm| {
            let pool = {
                vm.code.strings.push("TriggerSleepAction".into());
                (vm.code.strings.len() - 1) as u32
            };
            vm.code.functions[func as usize].code = vec![
                Instr::Int(1),
                Instr::StoreGlobal(phase_global),
                Instr::Real(0.5),
                Instr::Unknown { pool, argc: 1 },
                Instr::StoreGlobal(half_global),
                Instr::Int(2),
                Instr::StoreGlobal(phase_global),
                Instr::Real(-5.0),
                Instr::Unknown { pool, argc: 1 },
                Instr::StoreGlobal(negative_global),
                Instr::Int(3),
                Instr::StoreGlobal(phase_global),
                Instr::Real(0.0),
                Instr::Unknown { pool, argc: 1 },
                Instr::StoreGlobal(zero_global),
                Instr::Int(4),
                Instr::StoreGlobal(phase_global),
                Instr::Null,
                Instr::Return,
            ];
        })
        .expect("script attached");
    run(&mut indirect, "staged");
    assert_eq!(
        global_int(&indirect, "phase"),
        1,
        "the indirect TriggerSleepAction must suspend before the next statement"
    );
    for _ in 0..14 {
        indirect.step(&[]);
    }
    assert_eq!(global_int(&indirect, "phase"), 1, "0.5 s is 15 ticks");
    indirect.step(&[]);
    assert_eq!(global_int(&indirect, "phase"), 2, "resumed after the wait");
    assert!(
        global_real(&indirect, "half_slept") >= 0.49,
        "the indirect call returns the elapsed seconds: {}",
        global_real(&indirect, "half_slept")
    );
    // A negative timeout is a one-tick sleep, and the thread can park again.
    indirect.step(&[]);
    assert_eq!(global_int(&indirect, "phase"), 3, "a negative sleep wakes on the next tick");
    // Zero is also one tick, and the function then returns.
    indirect.step(&[]);
    assert_eq!(global_int(&indirect, "phase"), 4, "a zero sleep wakes on the next tick");
    assert!(
        global_real(&indirect, "negative_slept") > 0.0,
        "the negative sleep reports the elapsed tick: {}",
        global_real(&indirect, "negative_slept")
    );
    assert!(
        global_real(&indirect, "zero_slept") > 0.0,
        "the zero sleep reports the elapsed tick: {}",
        global_real(&indirect, "zero_slept")
    );
    assert!(
        indirect.script().expect("script").threads.is_empty(),
        "the finished thread is released"
    );
    assert_eq!(
        indirect.state_hash(),
        indirect_trial(0xA0D0_0172),
        "the same script and seed produce the same state hash (tick-driven, not wall-clock)"
    );
}

/// The indirect-path world of [`aud017_indirect_trigger_sleep_action_parks_the_thread`],
/// driven to the end of `staged`, for the determinism comparison.
fn indirect_trial(seed: u64) -> u64 {
    let mut sim = attach(
        seed,
        r#"
        globals
            integer phase = 0
            real half_slept = -1.0
            real negative_slept = -1.0
            real zero_slept = -1.0
        endglobals
        function staged takes nothing returns nothing
            set phase = 0
        endfunction
        "#,
        false,
    )
    .expect("headerless bundle compiles");
    let (func, phase_global, half_global, negative_global, zero_global) = {
        let code = &sim.script().expect("script").code;
        (
            code.by_name["staged"],
            code.global_by_name["phase"],
            code.global_by_name["half_slept"],
            code.global_by_name["negative_slept"],
            code.global_by_name["zero_slept"],
        )
    };
    sim.with_script(|vm| {
        let pool = {
            vm.code.strings.push("TriggerSleepAction".into());
            (vm.code.strings.len() - 1) as u32
        };
        vm.code.functions[func as usize].code = vec![
            Instr::Int(1),
            Instr::StoreGlobal(phase_global),
            Instr::Real(0.5),
            Instr::Unknown { pool, argc: 1 },
            Instr::StoreGlobal(half_global),
            Instr::Int(2),
            Instr::StoreGlobal(phase_global),
            Instr::Real(-5.0),
            Instr::Unknown { pool, argc: 1 },
            Instr::StoreGlobal(negative_global),
            Instr::Int(3),
            Instr::StoreGlobal(phase_global),
            Instr::Real(0.0),
            Instr::Unknown { pool, argc: 1 },
            Instr::StoreGlobal(zero_global),
            Instr::Int(4),
            Instr::StoreGlobal(phase_global),
            Instr::Null,
            Instr::Return,
        ];
    })
    .expect("script attached");
    run(&mut sim, "staged");
    for _ in 0..17 {
        sim.step(&[]);
    }
    sim.state_hash()
}

/// AUD-021: `SpatialHash::query` honours its documented limit at every value,
/// including zero, and rejects the degenerate radius inputs instead of returning
/// a bucket's contents.
#[test]
fn aud021_spatial_hash_query_honours_limit_and_degenerate_inputs() {
    use wc3_sim::spatial::SpatialHash;

    let mut hash = SpatialHash::new(64.0, 512.0, 512.0);
    let mut out = Vec::new();

    // Empty hash: nothing to return at any limit.
    hash.query(Vec2::new(100.0, 100.0), 200.0, 0, &mut out);
    assert!(out.is_empty(), "empty hash with limit 0");
    hash.query(Vec2::new(100.0, 100.0), 200.0, 8, &mut out);
    assert!(out.is_empty(), "empty hash with limit 8");

    for index in 0..5u32 {
        hash.insert(index, Vec2::new(100.0 + index as f32, 100.0));
    }
    out.push(999);
    hash.query(Vec2::new(100.0, 100.0), 200.0, 0, &mut out);
    assert!(
        out.is_empty(),
        "limit 0 must return zero neighbours, got {out:?}"
    );
    hash.query(Vec2::new(100.0, 100.0), 200.0, 1, &mut out);
    assert_eq!(out.len(), 1, "limit 1");
    hash.query(Vec2::new(100.0, 100.0), 200.0, 3, &mut out);
    assert_eq!(out.len(), 3, "limit 3");
    hash.query(Vec2::new(100.0, 100.0), 200.0, 8, &mut out);
    assert_eq!(out.len(), 5, "a limit above the population returns everything");
    hash.query(Vec2::new(100.0, 100.0), 200.0, usize::MAX, &mut out);
    assert_eq!(out.len(), 5, "usize::MAX is not special");

    // Radius semantics: only a positive finite circle selects buckets.
    for radius in [0.0f32, -1.0, f32::NAN, f32::INFINITY] {
        hash.query(Vec2::new(100.0, 100.0), radius, 8, &mut out);
        assert!(
            out.is_empty(),
            "radius {radius} is not a query circle, got {out:?}"
        );
    }
    hash.clear();
    hash.query(Vec2::new(100.0, 100.0), 200.0, 8, &mut out);
    assert!(out.is_empty(), "clear() empties the hash");
}

/// AUD-022: `trace_flow` validates the distance field against the grid before
/// indexing it. A short, empty, oversized or foreign field returns `None`
/// instead of panicking on an out-of-bounds index.
#[test]
fn aud022_trace_flow_rejects_an_inconsistent_distance_field() {
    use wc3_path::Grid;
    use wc3_sim::nav::trace_flow;

    let grid = Grid::new(12, 12, 128.0);
    let field = grid.flow_field((11, 11));
    assert_eq!(field.len(), 12 * 12, "the flow field is exactly one value per cell");
    let path = trace_flow(&grid, &field, (0, 0), 64).expect("a consistent field traces");
    assert_eq!(path.last(), Some(&(11, 11)));

    // One value short: the old code indexed past the end while descending.
    assert!(
        trace_flow(&grid, &field[..field.len() - 1], (0, 0), 64).is_none(),
        "a field one element short is rejected"
    );
    assert!(
        trace_flow(&grid, &[], (0, 0), 64).is_none(),
        "an empty field is rejected"
    );
    let mut oversized = field.clone();
    oversized.push(0);
    assert!(
        trace_flow(&grid, &oversized, (0, 0), 64).is_none(),
        "trailing data means the buffer is not this grid's field"
    );
    let smaller = Grid::new(3, 3, 128.0);
    assert!(
        trace_flow(&smaller, &field, (0, 0), 64).is_none(),
        "a field for another grid is rejected"
    );
    assert!(
        trace_flow(&grid, &field, (12, 0), 64).is_none(),
        "a start cell outside the grid is rejected"
    );
    assert!(
        trace_flow(&grid, &field, (0, 0), 0).is_some(),
        "a zero step budget still returns the start cell"
    );
}
