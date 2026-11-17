//! mechanic `jass.natives` — the five natives the shipped headers call that had
//! no handler (`jass_map_boot` / `jass_native_coverage` counted them as
//! unresolved). Each test boots a script through the real VM, calls the native,
//! and reads the effect back out of the machine state; nothing here asserts on
//! the handler table alone.

mod common;

use wc3_sim::Vec2;
use wc3_sim::jass::{ScriptBundle, ScriptedSim, Value, registry};

/// Boots `script.j` with the game headers (the script engine declares the
/// natives there). Enum-typed arguments are passed as their engine literals —
/// `ALLIANCE_PASSIVE` 0, `PLAYER_STATE_RESOURCE_GOLD` 1, `GREATER_THAN_OR_EQUAL`
/// 3, map-flag bits — so the tests do not depend on a header's constant names,
/// only on the values `common.j` assigns.
fn boot(script: &str) -> ScriptedSim {
    let mut bundle = ScriptBundle::new();
    if let Some(dir) = wc3_sim::jass::find_game_scripts() {
        bundle.add_game_scripts(&dir).expect("headers");
    }
    bundle.add_source("test.j", script).expect("parse");
    let mut sim = wc3_sim::Sim::new(0x1A55, 64, 64);
    sim.passive_income = false;
    sim.resources = [100, 100];
    sim.lumber = [100, 100];
    ScriptedSim::new(sim, &bundle, 0x1A55).expect("compile")
}

fn global_int(scripted: &ScriptedSim, name: &str) -> i64 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].int() as i64
}

fn global_string(scripted: &ScriptedSim, name: &str) -> String {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].text()
}

fn global_real(scripted: &ScriptedSim, name: &str) -> f32 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].real()
}

fn global_handle(scripted: &ScriptedSim, name: &str) -> wc3_sim::jass::Handle {
    let index = scripted.vm().code.global_by_name[name];
    match &scripted.vm().globals[index as usize] {
        Value::Handle(handle) => *handle,
        other => panic!("{name} is {other:?}, not a handle"),
    }
}

/// `war3map.wts` + `GlobalStrings.fdf` lookups, and the "never empty" rule for
/// a key that resolves nowhere.
#[test]
fn jass_natives_get_localized_string_reads_wts_then_globals_then_the_key() {
    assert!(
        registry::is_handled("GetLocalizedString"),
        "GetLocalizedString must have a handler"
    );
    let mut scripted = boot(
        r#"
        globals
            string from_map = ""
            string from_globals = ""
            string absent = ""
        endglobals
        function setup takes nothing returns nothing
            set from_map = GetLocalizedString("TRIGSTR_007")
            set from_globals = GetLocalizedString("PLAYER_LEFT_GAME")
            set absent = GetLocalizedString("NO_SUCH_KEY")
        endfunction
        "#,
    );
    // The map's own trigger-string table, then the install's global strings.
    assert_eq!(
        scripted
            .vm_mut()
            .host
            .strings
            .load_wts("STRING 7\n{\nfrom the map\n}\n"),
        1,
        "one wts entry"
    );
    let globals_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/war3_full3/UI/FrameDef/GlobalStrings.fdf");
    let globals_loaded = std::fs::read_to_string(&globals_path)
        .map(|text| scripted.vm_mut().host.strings.load_global_strings(&text))
        .unwrap_or(0);
    assert_eq!(
        scripted.vm_mut().host.strings.map_len(),
        1,
        "the wts table is installed"
    );

    scripted.vm_mut().run_function("setup").expect("setup");
    assert_eq!(global_string(&scripted, "from_map"), "from the map");
    if globals_loaded > 0 {
        let text = global_string(&scripted, "from_globals");
        assert_ne!(text, "PLAYER_LEFT_GAME", "the key must resolve");
        assert!(
            text.contains("%s"),
            "GlobalStrings.fdf entry for PLAYER_LEFT_GAME keeps its format slot: {text:?}"
        );
    } else {
        eprintln!("skip: GlobalStrings.fdf missing");
    }
    // A key that resolves nowhere comes back verbatim and is recorded once, so
    // a dialog never silently shows an empty string.
    assert_eq!(global_string(&scripted, "absent"), "NO_SUCH_KEY");
    let missing_line = scripted
        .vm()
        .host
        .strings
        .log
        .iter()
        .filter(|line| line.as_str() == "JASS_STRING missing=NO_SUCH_KEY")
        .count();
    assert_eq!(missing_line, 1, "the miss is logged exactly once");
    assert!(
        scripted
            .vm()
            .host
            .strings
            .missing_keys()
            .contains(&"NO_SUCH_KEY"),
        "the missing key is recorded"
    );
}

/// `GroupEnumUnitsInRangeOfLoc` fills the group by distance, honours the
/// boolexpr filter, and emits members in unit-id order.
#[test]
fn jass_natives_group_enum_units_in_range_of_loc_filters_by_distance_and_order() {
    assert!(
        registry::is_handled("GroupEnumUnitsInRangeOfLoc"),
        "GroupEnumUnitsInRangeOfLoc must have a handler"
    );
    // Units are created by the script (`CreateUnit`), which binds their
    // handles — the same path a map takes.
    let mut scripted = boot(
        r#"
        globals
            group g = null
            integer members = 0
            real first_x = 0.0
            real second_x = 0.0
        endglobals
        function only_footmen takes nothing returns boolean
            return GetUnitTypeId(GetFilterUnit()) == 'hfoo'
        endfunction
        function setup takes nothing returns nothing
            local integer i = 0
            local unit u
            call CreateUnit(Player(0), 'hfoo', 1000.0, 1150.0, 0.0)
            call CreateUnit(Player(0), 'ogru', 950.0, 1050.0, 0.0)
            call CreateUnit(Player(0), 'hfoo', 1150.0, 1000.0, 0.0)
            call CreateUnit(Player(0), 'hfoo', 2000.0, 2000.0, 0.0)
            set g = CreateGroup()
            call GroupEnumUnitsInRangeOfLoc(g, Location(1000.0, 1000.0), 300.0, Filter(function only_footmen))
            loop
                set u = FirstOfGroup(g)
                exitwhen u == null
                if i == 0 then
                    set first_x = GetUnitX(u)
                elseif i == 1 then
                    set second_x = GetUnitX(u)
                endif
                set i = i + 1
            endloop
            set members = i
            // `FirstOfGroup` drains the group; fill it again so the harness
            // can inspect the members the native produced.
            call GroupEnumUnitsInRangeOfLoc(g, Location(1000.0, 1000.0), 300.0, Filter(function only_footmen))
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");

    // The filter drops the grunt; the radius drops the far footman.
    assert_eq!(global_int(&scripted, "members"), 2, "two footmen in range");

    let group = global_handle(&scripted, "g");
    let members: Vec<u32> = scripted
        .vm()
        .host
        .group(group)
        .map(|members| members.clone())
        .expect("group object");
    assert_eq!(members.len(), 2, "group members");
    let handles = scripted.vm().host.units.clone();
    let units: Vec<_> = members
        .iter()
        .map(|id| handles.unit_of(*id).expect("bound unit"))
        .collect();
    // Unit-id order, and both members are footmen inside the radius.
    let unit_ids: Vec<u32> = units.iter().map(|unit| unit.0).collect();
    assert!(
        unit_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "members are emitted in unit-id order: {unit_ids:?}"
    );
    let mut xs: Vec<f32> = Vec::new();
    for unit in &units {
        let spawned = scripted.sim().unit(*unit).expect("unit");
        assert_eq!(spawned.def().id, "hfoo", "the filter excluded the grunt");
        assert!(
            spawned.pos.distance(Vec2::new(1000.0, 1000.0)) <= 300.0,
            "member {:?} is inside the radius",
            spawned.pos
        );
        xs.push(spawned.pos.x);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(xs, vec![1000.0, 1150.0], "the two near footmen");
    // The script saw the same two, in the same order.
    let first_x = global_real(&scripted, "first_x");
    let second_x = global_real(&scripted, "second_x");
    assert_eq!([first_x, second_x], [1000.0, 1150.0], "script-visible order");
}

/// Builds a minimal `war3map.w3i` (v18) with a known flags word.
fn w3i_v18(flags: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&18u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&6059u32.to_le_bytes());
    for text in ["name", "author", "description", "recommended"] {
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
    }
    bytes.extend_from_slice(&[0u8; 32]); // camera bounds (8 floats)
    bytes.extend_from_slice(&[0u8; 16]); // bounds complements (4 ints)
    bytes.extend_from_slice(&[0u8; 8]); // playable width/height
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes
}

/// `IsMapFlagSet` answers from the loaded `war3map.w3i` word, and a value that
/// is not an engine bit is `false` plus a recorded gap.
#[test]
fn jass_natives_is_map_flag_set_reads_the_loaded_w3i_word() {
    assert!(
        registry::is_handled("IsMapFlagSet"),
        "IsMapFlagSet must have a handler"
    );
    let mut scripted = boot(
        r#"
        globals
            boolean hide_minimap = false
            boolean unknown = false
            boolean not_set = false
        endglobals
        function setup takes nothing returns nothing
            set hide_minimap = IsMapFlagSet(1)
            set unknown = IsMapFlagSet(5)
            set not_set = IsMapFlagSet(64)
        endfunction
        "#,
    );
    // HideMinimapInPreviewScreens (bit 0) and MeleeMap (bit 2).
    let mut bundle = ScriptBundle::new();
    assert!(
        bundle.add_map_w3i(&w3i_v18(0b101)),
        "the v18 header layout is decodable"
    );
    scripted.vm_mut().host.map_flags = bundle.map_flags;

    scripted.vm_mut().run_function("setup").expect("setup");
    let read = |name: &str| -> bool {
        let index = scripted.vm().code.global_by_name[name];
        scripted.vm().globals[index as usize].truthy()
    };
    assert!(read("hide_minimap"), "bit 0 is set in the header");
    // 5 = bits 0|2, not a single engine flag: false, and the gap is recorded.
    assert!(!read("unknown"), "a non-bit value is never true");
    assert!(
        scripted.vm().host.unknown_map_flags.contains(&5),
        "the unknown map flag is recorded: {:?}",
        scripted.vm().host.unknown_map_flags
    );
    assert!(!read("not_set"), "bit 6 is clear in the header");
}

/// `TriggerRegisterPlayerAllianceChange` fires when the seat's alliance state
/// changes, once per change, and not for a repeated assignment.
#[test]
fn jass_natives_trigger_register_player_alliance_change_fires_on_a_real_change() {
    assert!(
        registry::is_handled("TriggerRegisterPlayerAllianceChange"),
        "TriggerRegisterPlayerAllianceChange must have a handler"
    );
    let mut scripted = boot(
        r#"
        globals
            trigger t = null
            integer fires = 0
        endglobals
        function on_change takes nothing returns nothing
            set fires = fires + 1
        endfunction
        function setup takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function on_change)
            // Different teams: the melee default is "not allied", so setting
            // the alliance below is a real transition.
            call SetPlayerTeam(Player(0), 0)
            call SetPlayerTeam(Player(1), 1)
            call TriggerRegisterPlayerAllianceChange(t, Player(0), 0)
        endfunction
        function ally takes nothing returns nothing
            call SetPlayerAlliance(Player(0), Player(1), 0, true)
        endfunction
        function unally takes nothing returns nothing
            call SetPlayerAlliance(Player(0), Player(1), 0, false)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    // No change yet: the registration alone does not fire.
    for _ in 0..3 {
        scripted.step(&[]);
    }
    assert_eq!(global_int(&scripted, "fires"), 0, "no change, no event");

    scripted.vm_mut().run_function("ally").expect("ally");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 1, "the change fires once");

    // Re-assigning the same value is not a change.
    scripted.vm_mut().run_function("ally").expect("ally again");
    for _ in 0..3 {
        scripted.step(&[]);
    }
    assert_eq!(
        global_int(&scripted, "fires"),
        1,
        "a repeated assignment must not fire"
    );

    scripted.vm_mut().run_function("unally").expect("unally");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 2, "the reverse change fires");
}

/// `TriggerRegisterPlayerStateEvent` fires when the state crosses the
/// comparison, sampled once per tick: down is not a fire, and staying above the
/// limit is not a second fire.
#[test]
fn jass_natives_trigger_register_player_state_event_fires_on_the_crossing() {
    assert!(
        registry::is_handled("TriggerRegisterPlayerStateEvent"),
        "TriggerRegisterPlayerStateEvent must have a handler"
    );
    let mut scripted = boot(
        r#"
        globals
            trigger t = null
            integer fires = 0
        endglobals
        function on_limit takes nothing returns nothing
            set fires = fires + 1
        endfunction
        function setup takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function on_limit)
            // PLAYER_STATE_RESOURCE_GOLD (1), GREATER_THAN_OR_EQUAL (3)
            call TriggerRegisterPlayerStateEvent(t, Player(0), 1, 3, 500.0)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    scripted.sim_mut().resources[0] = 100;
    for _ in 0..2 {
        scripted.step(&[]);
    }
    assert_eq!(global_int(&scripted, "fires"), 0, "below the limit");

    scripted.sim_mut().resources[0] = 600;
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 1, "crossing up fires once");
    for _ in 0..5 {
        scripted.step(&[]);
    }
    assert_eq!(
        global_int(&scripted, "fires"),
        1,
        "staying above the limit does not re-fire"
    );

    scripted.sim_mut().resources[0] = 200;
    scripted.step(&[]);
    assert_eq!(
        global_int(&scripted, "fires"),
        1,
        "crossing down is not a fire"
    );

    scripted.sim_mut().resources[0] = 900;
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 2, "a second crossing fires");
}
