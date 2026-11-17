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

// ---------------------------------------------------------------------------
// AUD-005 — `GetSpellAbilityId` must carry the cast spell's rawcode
// ---------------------------------------------------------------------------

/// Every `Spell` variant maps to the four-character ability id its data row
/// carries, and `spell_rawcode` converts it the way the engine stores a rawcode
/// (`'AHhb'` = `0x41486862`). The table is the honest `Spell -> ability id`
/// mapping derived from `SpellDef`/`AbilityData`, never the `Debug` name.
#[test]
fn jass_natives_spell_rawcode_covers_every_variant() {
    // Typed spells: the id comes from the `SpellDef` row.
    for spell in wc3_sim::Spell::ALL {
        let id = spell.ability_id();
        assert_eq!(id.len(), 4, "{spell:?} ability id {id:?} is a rawcode");
        let expected =
            i32::from_be_bytes([id.as_bytes()[0], id.as_bytes()[1], id.as_bytes()[2], id.as_bytes()[3]]);
        let raw = wc3_sim::jass::natives::spell_rawcode(spell);
        assert_ne!(raw, 0, "{spell:?} must not answer rawcode 0");
        assert_eq!(raw, expected, "{spell:?} rawcode");
        assert_eq!(
            wc3_sim::jass::natives::unit_rawcode(id),
            raw,
            "unit_rawcode and spell_rawcode agree on {id}"
        );
    }
    // Data-backed variants carry their id; an id that is absent from the sheet
    // never becomes a `Spell`, so every constructible variant has one.
    let data_variants = [
        wc3_sim::Spell::DataDamage { id: "AHtb" },
        wc3_sim::Spell::DataHeal { id: "AHea" },
        wc3_sim::Spell::DataBuff { id: "AHfa" },
        wc3_sim::Spell::DataSummon { id: "AHwe" },
        wc3_sim::Spell::DataAura { id: "AHad" },
        wc3_sim::Spell::DataAuraPassive { id: "AOae" },
        wc3_sim::Spell::DataTransformation { id: "AEme" },
        wc3_sim::Spell::DataOnly { id: "AInv" },
    ];
    for spell in data_variants {
        let id = spell.ability_id();
        assert_eq!(
            wc3_sim::jass::natives::spell_rawcode(spell),
            wc3_sim::jass::natives::unit_rawcode(id),
            "{spell:?}"
        );
        assert_ne!(wc3_sim::jass::natives::spell_rawcode(spell), 0, "{spell:?}");
    }
}

/// The scheduler puts the rawcode into `EventContext.spell_id`, so a JASS
/// action reads the real ability id through `GetSpellAbilityId`.
#[test]
fn jass_natives_spell_cast_event_context_carries_ability_id() {
    let mut scripted = boot(
        r#"
        globals
            unit caster = null
            trigger t = null
            integer casts = 0
            integer spell_id = 0
            integer spell_event_id = 0
        endglobals
        function on_cast takes nothing returns nothing
            set casts = casts + 1
            set spell_id = GetSpellAbilityId()
        endfunction
        function setup takes nothing returns nothing
            set caster = CreateUnit(Player(0), 'hfoo', 1024.0, 1024.0, 0.0)
            set t = CreateTrigger()
            call TriggerAddAction(t, function on_cast)
            call TriggerRegisterUnitEvent(t, caster, ConvertUnitEvent(spell_event_id))
        endfunction
        "#,
    );
    // The event id comes from the loaded header (1.32.9: 291), not a literal:
    // the engine resolves the same constant, so the registration cannot drift.
    let spell_event_id = scripted.vm().event_ids.unit_spell;
    let slot = scripted.vm().code.global_by_name["spell_event_id"] as usize;
    scripted.vm_mut().globals[slot] = Value::Int(spell_event_id as i32);
    scripted.vm_mut().run_function("setup").expect("setup");
    let caster = scripted
        .vm()
        .host
        .sim_unit(global_handle(&scripted, "caster"))
        .expect("the script-created unit is a simulation unit");
    // The simulation raising the cast is what a real cast produces.
    scripted.sim_mut().events.push(wc3_sim::Event::SpellCast {
        caster,
        spell: wc3_sim::Spell::HolyLight,
        target: wc3_sim::SpellTarget::Unit(caster),
    });
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "casts"), 1, "the event fired once");
    let expected = i32::from_be_bytes(*b"AHhb");
    assert_eq!(
        global_int(&scripted, "spell_id"),
        i64::from(expected),
        "GetSpellAbilityId returns the ability rawcode, not 0"
    );
}

// ---------------------------------------------------------------------------
// AUD-006 — game state / game event registrations
// ---------------------------------------------------------------------------

/// `TriggerRegisterGameStateEvent` is a real per-tick watch: a limit crossing
/// fires it once, the same crossing does not re-fire, leaving the limit and
/// crossing again fires again, and disable/enable/destroy are honoured.
#[test]
fn jass_natives_game_state_trigger_fires_across_a_threshold() {
    let mut scripted = boot(
        r#"
        globals
            trigger t = null
            integer fires = 0
            integer last_event = 0
            integer last_state = -1
        endglobals
        function action takes nothing returns nothing
            set fires = fires + 1
            set last_event = GetTriggerEventId()
            set last_state = GetEventGameState()
        endfunction
        function setup takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            // GAME_STATE_TIME_OF_DAY is fgamestate 2 in common.j 1.32.9;
            // GREATER_THAN is limitop 4.
            call TriggerRegisterGameStateEvent(t, ConvertFGameState(2), ConvertLimitOp(4), 10.0)
        endfunction
        function set_hour takes nothing returns nothing
            call SetFloatGameState(ConvertFGameState(2), 3.0)
        endfunction
        function set_night takes nothing returns nothing
            call SetFloatGameState(ConvertFGameState(2), 21.0)
        endfunction
        function disable takes nothing returns nothing
            call DisableTrigger(t)
        endfunction
        function enable takes nothing returns nothing
            call EnableTrigger(t)
        endfunction
        function destroy takes nothing returns nothing
            call DestroyTrigger(t)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 0, "no state set yet");

    scripted.vm_mut().run_function("set_night").expect("set_night");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 1, "crossing up fires once");
    for _ in 0..4 {
        scripted.step(&[]);
    }
    assert_eq!(
        global_int(&scripted, "fires"),
        1,
        "staying above the limit does not re-fire"
    );
    assert_eq!(
        global_int(&scripted, "last_event"),
        3,
        "the context carries EVENT_GAME_STATE_LIMIT (ConvertGameEvent(3))"
    );
    assert_eq!(
        global_int(&scripted, "last_state"),
        2,
        "GetEventGameState names the gamestate that fired (fgamestate 2)"
    );

    scripted.vm_mut().run_function("set_hour").expect("set_hour");
    scripted.step(&[]);
    assert_eq!(
        global_int(&scripted, "fires"),
        1,
        "crossing down is not a fire"
    );

    scripted.vm_mut().run_function("disable").expect("disable");
    scripted.vm_mut().run_function("set_night").expect("set_night");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 1, "disabled trigger is silent");

    scripted.vm_mut().run_function("enable").expect("enable");
    scripted.vm_mut().run_function("set_hour").expect("set_hour");
    scripted.step(&[]);
    scripted.vm_mut().run_function("set_night").expect("set_night");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 2, "re-enabled trigger fires");

    scripted.vm_mut().run_function("destroy").expect("destroy");
    scripted.vm_mut().run_function("set_hour").expect("set_hour");
    scripted.step(&[]);
    scripted.vm_mut().run_function("set_night").expect("set_night");
    scripted.step(&[]);
    assert_eq!(
        global_int(&scripted, "fires"),
        2,
        "a destroyed trigger never fires again"
    );
}

/// `TriggerRegisterGameEvent` stores a real registration and returns the engine's
/// per-id event object, and a crossing of a game-state limit also raises
/// `EVENT_GAME_STATE_LIMIT` for it.
#[test]
fn jass_natives_game_event_registration_fires_with_the_engine_event() {
    let mut scripted = boot(
        r#"
        globals
            trigger state_trigger = null
            trigger game_trigger = null
            event state_event = null
            event game_event_handle = null
            integer state_fires = 0
            integer game_fires = 0
            integer game_event_id = -1
        endglobals
        function on_state takes nothing returns nothing
            set state_fires = state_fires + 1
        endfunction
        function on_game takes nothing returns nothing
            set game_fires = game_fires + 1
            set game_event_id = GetTriggerEventId()
        endfunction
        function setup takes nothing returns nothing
            set state_trigger = CreateTrigger()
            call TriggerAddAction(state_trigger, function on_state)
            set state_event = TriggerRegisterGameStateEvent(state_trigger, ConvertFGameState(2), ConvertLimitOp(4), 10.0)
            set game_trigger = CreateTrigger()
            call TriggerAddAction(game_trigger, function on_game)
            // EVENT_GAME_STATE_LIMIT = ConvertGameEvent(3)
            set game_event_handle = TriggerRegisterGameEvent(game_trigger, ConvertGameEvent(3))
        endfunction
        function set_night takes nothing returns nothing
            call SetFloatGameState(ConvertFGameState(2), 21.0)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    // Both registrations returned real event objects (not a synthetic handle).
    let state_event = global_handle(&scripted, "state_event");
    let game_event = global_handle(&scripted, "game_event_handle");
    assert!(
        matches!(
            scripted.vm().host.object(state_event),
            Some(wc3_sim::jass::Object::GameEvent { id: 3 })
        ),
        "the game-state registration returns the EVENT_GAME_STATE_LIMIT object"
    );
    assert!(
        matches!(
            scripted.vm().host.object(game_event),
            Some(wc3_sim::jass::Object::GameEvent { id: 3 })
        ),
        "TriggerRegisterGameEvent returns the event object for its id"
    );
    assert_eq!(
        state_event.id, game_event.id,
        "one event object per gameevent id, like the engine's global events"
    );

    scripted.vm_mut().run_function("set_night").expect("set_night");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "state_fires"), 1, "state trigger");
    assert_eq!(
        global_int(&scripted, "game_fires"),
        1,
        "the limit crossing is an EVENT_GAME_STATE_LIMIT game event"
    );
    assert_eq!(global_int(&scripted, "game_event_id"), 3, "event id");
}

/// A game event the engine has no subsystem for is registered, reported as
/// `Partial` and delivered when the host raises it — never a silent no-op.
#[test]
fn jass_natives_game_event_without_an_engine_emitter_is_registered_and_partial() {
    let mut scripted = boot(
        r#"
        globals
            trigger t = null
            integer fires = 0
            integer seen = -1
        endglobals
        function action takes nothing returns nothing
            set fires = fires + 1
            set seen = GetTriggerEventId()
        endfunction
        function setup takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            // EVENT_GAME_VICTORY = ConvertGameEvent(0)
            call TriggerRegisterGameEvent(t, ConvertGameEvent(0))
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    let index = scripted.vm().code.global_by_name["t"] as usize;
    let handle = match scripted.vm().globals[index].clone() {
        Value::Handle(handle) => handle,
        other => panic!("t is {other:?}"),
    };
    assert_eq!(
        scripted
            .vm()
            .host
            .trigger(handle)
            .map(|state| state.events.len()),
        Some(1),
        "the registration is stored on the trigger"
    );
    assert!(
        scripted.vm().host.raise_only_game_events.contains(&0),
        "a game event with no engine emitter is reported as Partial"
    );

    scripted.vm_mut().host.raise_game_event(0);
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "fires"), 1, "the raised event fired");
    assert_eq!(global_int(&scripted, "seen"), 0, "the event id is carried");
}

// ---------------------------------------------------------------------------
// AUD-007 — dialog / timer-dialog headless state
// ---------------------------------------------------------------------------

/// `DialogCreate` → `DialogSetMessage` → `DialogAddButton`/`DialogAddQuitButton`
/// → `DialogDisplay(true/false)` → `DialogDestroy` all reach a state a headless
/// test can read, and destroy frees the dialog and its buttons.
#[test]
fn jass_natives_dialog_state_round_trip() {
    let mut scripted = boot(
        r#"
        globals
            dialog d = null
            button first = null
            button quit = null
        endglobals
        function setup takes nothing returns nothing
            set d = DialogCreate()
            call DialogSetMessage(d, "Choose your hero")
            set first = DialogAddButton(d, "Archmage", 0)
            set quit = DialogAddQuitButton(d, false, "Quit", 0)
            call DialogSetAsync(d)
            call DialogDisplay(Player(0), d, true)
        endfunction
        function hide takes nothing returns nothing
            call DialogDisplay(Player(0), d, false)
        endfunction
        function destroy takes nothing returns nothing
            call DialogDestroy(d)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    let dialog = global_handle(&scripted, "d");
    match scripted.vm().host.object(dialog) {
        Some(wc3_sim::jass::Object::Dialog {
            message,
            shown_to,
            asynchronous,
            buttons,
        }) => {
            assert_eq!(message, "Choose your hero", "the message is stored");
            assert_eq!(*shown_to, Some(0), "DialogDisplay recorded the player");
            assert!(*asynchronous, "DialogSetAsync is recorded");
            assert_eq!(buttons.len(), 2, "two buttons");
        }
        other => panic!("dialog object is {other:?}"),
    }
    let first = global_handle(&scripted, "first");
    let quit = global_handle(&scripted, "quit");
    assert!(
        matches!(
            scripted.vm().host.object(first),
            Some(wc3_sim::jass::Object::Button { text, quit: false, .. }) if text == "Archmage"
        ),
        "a normal button keeps its text and is not a quit button"
    );
    assert!(
        matches!(
            scripted.vm().host.object(quit),
            Some(wc3_sim::jass::Object::Button { text, quit: true, .. }) if text == "Quit"
        ),
        "DialogAddQuitButton is marked"
    );

    scripted.vm_mut().run_function("hide").expect("hide");
    assert!(
        matches!(
            scripted.vm().host.object(dialog),
            Some(wc3_sim::jass::Object::Dialog { shown_to: None, .. })
        ),
        "DialogDisplay(false) hides the dialog"
    );

    let before = scripted.vm().host.objects.len();
    scripted.vm_mut().run_function("destroy").expect("destroy");
    assert!(
        scripted.vm().host.object(dialog).is_none(),
        "DialogDestroy frees the dialog"
    );
    assert!(
        scripted.vm().host.object(first).is_none() && scripted.vm().host.object(quit).is_none(),
        "DialogDestroy frees the buttons it owns"
    );
    assert_eq!(
        scripted.vm().host.objects.len(),
        before - 3,
        "exactly the dialog and its two buttons were freed"
    );
    assert!(
        scripted.vm().errors.is_empty(),
        "no diagnostics for a valid dialog sequence: {:?}",
        scripted.vm().errors
    );
}

/// Timer dialogs: title, display flag, speed, colours and the real-time
/// remaining override are state the HUD reads back; destroy frees the object.
#[test]
fn jass_natives_timer_dialog_state_round_trip() {
    let mut scripted = boot(
        r#"
        globals
            timer tm = null
            timerdialog td = null
        endglobals
        function setup takes nothing returns nothing
            set tm = CreateTimer()
            call TimerStart(tm, 30.0, false, null)
            set td = CreateTimerDialog(tm)
            call TimerDialogSetTitle(td, "Wave 1")
            call TimerDialogSetSpeed(td, 2.0)
            call TimerDialogSetRealTimeRemaining(td, 12.5)
            call TimerDialogSetTimeColor(td, 10, 20, 30, 255)
            call TimerDialogSetTitleColor(td, 1, 2, 3, 128)
            call TimerDialogDisplay(td, true)
        endfunction
        function hide takes nothing returns nothing
            call TimerDialogDisplay(td, false)
        endfunction
        function destroy takes nothing returns nothing
            call DestroyTimerDialog(td)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    let dialog = global_handle(&scripted, "td");
    let timer = global_handle(&scripted, "tm");
    match scripted.vm().host.object(dialog) {
        Some(wc3_sim::jass::Object::TimerDialog {
            timer: bound,
            title,
            shown,
            speed,
            remaining_override,
            time_color,
            title_color,
        }) => {
            assert_eq!(*bound, timer.id, "bound to CreateTimerDialog's timer");
            assert_eq!(title, "Wave 1");
            assert!(*shown, "TimerDialogDisplay(true)");
            assert_eq!(*speed, 2.0);
            assert_eq!(*remaining_override, Some(12.5));
            assert_eq!(*time_color, [10, 20, 30, 255]);
            assert_eq!(*title_color, [1, 2, 3, 128]);
        }
        other => panic!("timer dialog is {other:?}"),
    }
    scripted.vm_mut().run_function("hide").expect("hide");
    assert!(
        matches!(
            scripted.vm().host.object(dialog),
            Some(wc3_sim::jass::Object::TimerDialog { shown: false, .. })
        ),
        "TimerDialogDisplay(false) hides it"
    );
    scripted.vm_mut().run_function("destroy").expect("destroy");
    assert!(
        scripted.vm().host.object(dialog).is_none(),
        "DestroyTimerDialog frees it"
    );
    assert!(
        scripted.vm().errors.is_empty(),
        "no diagnostics: {:?}",
        scripted.vm().errors
    );
}

/// A dialog call against a wrong handle is a diagnostic + null, never a silent
/// success (`DialogSetMessage` on a timer must not look like it worked).
#[test]
fn jass_natives_dialog_wrong_handle_is_a_diagnostic_not_a_silent_success() {
    let mut scripted = boot(
        r#"
        globals
            timer tm = null
            dialog d = null
        endglobals
        function setup takes nothing returns nothing
            set tm = CreateTimer()
            set d = DialogCreate()
            call DialogSetMessage(tm, "not a dialog")
            call DialogDisplay(Player(0), d, true)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    assert!(
        scripted
            .vm()
            .errors
            .iter()
            .any(|error| error.contains("DialogSetMessage") && error.contains("not a dialog")),
        "the wrong handle is reported: {:?}",
        scripted.vm().errors
    );
    assert!(
        matches!(
            scripted.vm().host.object(global_handle(&scripted, "d")),
            Some(wc3_sim::jass::Object::Dialog { message, .. }) if message.is_empty()
        ),
        "the real dialog keeps its empty message"
    );
}

/// The headerless defaults in `EventIds::default()` are the values of the
/// shipped `common.j`: parsed out of the file, so a typo cannot hide behind the
/// runtime override (`from_vm` reads the same constants from a loaded header).
#[test]
fn jass_natives_event_id_defaults_match_the_shipped_header() {
    let Some(dir) = wc3_sim::jass::find_game_scripts() else {
        eprintln!("skip: no game headers in this checkout");
        return;
    };
    let header = std::fs::read_to_string(dir.join("common.j")).expect("common.j");
    let value = |name: &str| -> i64 {
        let needle = format!("{name} ");
        let line = header
            .lines()
            .find(|line| line.trim_start().starts_with("constant") && line.contains(&needle))
            .unwrap_or_else(|| panic!("header does not declare {name}"));
        let open = line.find('(').expect("ConvertXEvent(");
        let close = line[open..].find(')').expect(")");
        line[open + 1..open + close].trim().parse().expect("number")
    };
    let ids = wc3_sim::jass::scheduler::EventIds::default();
    let pairs: [(&str, i64); 22] = [
        ("EVENT_UNIT_DEATH", ids.unit_death),
        ("EVENT_UNIT_DAMAGED", ids.unit_damaged),
        ("EVENT_UNIT_TRAIN_FINISH", ids.unit_trained),
        ("EVENT_UNIT_CONSTRUCT_FINISH", ids.unit_construct),
        ("EVENT_UNIT_RESEARCH_FINISH", ids.unit_researched),
        ("EVENT_UNIT_HERO_LEVEL", ids.unit_hero_level),
        ("EVENT_UNIT_SPELL_EFFECT", ids.unit_spell),
        ("EVENT_UNIT_ISSUED_ORDER", ids.unit_orders),
        ("EVENT_UNIT_CHANGE_OWNER", ids.unit_change_owner),
        ("EVENT_UNIT_ATTACKED", ids.unit_attacked),
        ("EVENT_PLAYER_UNIT_DEATH", ids.player_unit_death),
        ("EVENT_PLAYER_UNIT_DAMAGED", ids.player_unit_damaged),
        ("EVENT_PLAYER_UNIT_TRAIN_FINISH", ids.player_unit_trained),
        ("EVENT_PLAYER_UNIT_CONSTRUCT_FINISH", ids.player_unit_construct),
        ("EVENT_PLAYER_UNIT_RESEARCH_FINISH", ids.player_unit_researched),
        ("EVENT_PLAYER_HERO_LEVEL", ids.player_hero_level),
        ("EVENT_PLAYER_UNIT_SPELL_EFFECT", ids.player_unit_spell),
        ("EVENT_PLAYER_UNIT_ISSUED_ORDER", ids.player_unit_orders),
        ("EVENT_PLAYER_UNIT_CHANGE_OWNER", ids.player_unit_change_owner),
        ("EVENT_PLAYER_UNIT_ATTACKED", ids.player_unit_attacked),
        ("EVENT_PLAYER_CHAT", ids.player_chat),
        ("EVENT_GAME_STATE_LIMIT", ids.game_state_limit),
    ];
    for (name, default) in pairs {
        assert_eq!(
            default,
            value(name),
            "{name}: EventIds::default() must equal the shipped header"
        );
    }
}
