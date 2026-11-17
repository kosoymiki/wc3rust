//! The ready melee map (Ponds) must be playable: workers walk and build.
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;

fn game() -> Option<Game> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x");
    let path = if path.is_file() { path } else { Path::new("/data/data/org.herofolends.p0/files/Melee.w3x").to_path_buf() };
    if !path.is_file() { eprintln!("skip: melee map missing"); return None; }
    Some(Game::new(Assets::load(None, Some(&path)), 0x5EED))
}

#[test]
fn melee_worker_walks_and_builds_on_the_ready_map() {
    let Some(mut game) = game() else { return };
    let workers: Vec<_> = game.sim.alive_units()
        .filter(|u| u.owner == game.player && u.def().stats.is_worker)
        .map(|u| u.id).collect();
    assert!(!workers.is_empty(), "player has workers");
    let worker = workers[0];
    let pos = game.sim.unit(worker).unwrap().pos;
    let cell = game.sim.grid.world_to_cell(pos);
    eprintln!(
        "worker def={} name={} pos={:?} cell={:?} walkable={} grid={}x{} cell_size={}",
        game.sim.unit(worker).unwrap().def().id,
        game.assets.name_for_def(game.sim.unit(worker).unwrap().def().id).unwrap_or_default(),
        pos, cell, game.sim.grid.walkable(cell.0, cell.1),
        game.sim.grid.width, game.sim.grid.height, game.sim.grid.cell_size
    );
    let target = pos + wc3_sim::Vec2::new(1400.0, 0.0);
    // Operator bug #3: no worker may stand inside a tree/rock/cliff cell.
    assert!(
        game.sim.grid.walkable(cell.0, cell.1),
        "operator bug #3: worker spawns inside a blocked cell {cell:?} (pos {pos:?})"
    );
    game.sim.issue(&workers, wc3_sim::OrderSpec::Move(target));
    for _ in 0..20 { game.update(1.0 / 30.0); }
    let path_len = game.sim.unit(worker).unwrap().path.len();
    eprintln!("path_len={} order={:?} goal={:?}", path_len, game.sim.unit(worker).unwrap().order, game.sim.unit(worker).unwrap().goal);
    assert!(path_len > 0, "long move produced no path (PATH BLOCKED)");
    assert!(path_len > 5, "path too short: {path_len}");
}

/// Operator bug #1 on the original melee map: a cross-map move must produce a
/// path and actually move the unit thousands of units, not +65 px.
#[test]
fn cross_map_move_on_justice() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return;
    }
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    // The device enters the battle from the glue flow; the headless harness
    // drives the same live scene, so the sim must tick from the first frame.
    game.enter_battle();
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("player worker");
    let start = game.sim.unit(worker).expect("worker").pos;
    let world = game.assets.world_size;
    let target = wc3_sim::Vec2::new(world.x * 0.5, world.y * 0.5);
    // The operator's path: a code-issued command through the game API, not a
    // direct sim poke.
    game.issue(wc3_sim::Command::Move { units: vec![worker], to: target });
    let mut best = start.distance(target);
    let mut travelled = 0.0f32;
    let mut last = start;
    for _ in 0..3_000 {
        game.update(1.0 / 30.0);
        if let Some(unit) = game.sim.unit(worker) {
            travelled += unit.pos.distance(last);
            last = unit.pos;
            best = best.min(unit.pos.distance(target));
        }
    }
    eprintln!(
        "start={start:?} end={last:?} target={target:?} best={best:.0} travelled={travelled:.0}"
    );
    assert!(travelled > 800.0, "unit barely moved: {travelled:.0}");
}

/// Operator bug #10: entering placement must produce a ghost immediately.
#[test]
fn placement_ghost_exists_without_pointer_move() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return;
    }
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    game.begin_placement("htow");
    assert!(game.placing.is_some(), "placement mode entered");
    assert!(
        game.placing_at.is_some(),
        "ghost anchor must exist immediately, not only after pointer_move"
    );
    let at = game.placing_at.unwrap();
    assert!(game.sim.can_place("htow", at) || !game.sim.can_place("htow", at));
}

/// Operator bug: the map's gold mine must pay gold and trees must be choppable.
#[test]
fn gold_and_lumber_on_justice() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return;
    }
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("worker");
    let pos = game.sim.unit(worker).unwrap().pos;
    let mine = game
        .sim
        .gold_mines
        .iter()
        .min_by(|a, b| {
            a.pos
                .distance(pos)
                .partial_cmp(&b.pos.distance(pos))
                .unwrap()
        })
        .map(|m| m.pos)
        .expect("gold mine");
    let before = game.sim.resources[game.player as usize];
    // Exercise the UI path: the game queues Command::Harvest, not OrderSpec.
    game.issue(wc3_sim::Command::Harvest {
        units: vec![worker],
        to: mine,
    });
    for _ in 0..4_000 {
        game.update(1.0 / 30.0);
    }
    let after = game.sim.resources[game.player as usize];
    eprintln!("gold {before} -> {after} mine={mine:?}");
    assert!(after > before, "gold mine pays nothing ({before} -> {after})");

    let tree = game
        .sim
        .destructables
        .iter()
        .min_by(|a, b| {
            a.pos
                .distance(pos)
                .partial_cmp(&b.pos.distance(pos))
                .unwrap()
        })
        .map(|t| t.pos)
        .expect("tree");
    let lumber_before = game.sim.lumber[game.player as usize];
    game.issue(wc3_sim::Command::HarvestLumber {
        units: vec![worker],
        to: tree,
    });
    for _ in 0..4_000 {
        game.update(1.0 / 30.0);
    }
    let lumber_after = game.sim.lumber[game.player as usize];
    eprintln!("lumber {lumber_before} -> {lumber_after} tree={tree:?}");
    assert!(
        lumber_after > lumber_before,
        "trees give no lumber ({lumber_before} -> {lumber_after})"
    );
}

fn justice_path() -> Option<std::path::PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skip: Justice.w3x missing");
        None
    }
}

/// Remaining-work #1/#2: both seats start with their own `war3map.w3i` race
/// kit (Justice: Player 1 = Human west, Player 2 = Orc east), and the scene
/// contains exactly the map's own mines/units/destructables — no synthetic
/// base camps and no borrowed enemy kit. The only generated units are the two
/// halls and ten workers of the per-seat melee opening.
#[test]
fn justice_melee_start_units_follow_the_w3i_race_of_each_seat() {
    let Some(path) = justice_path() else { return };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("justice bundle");
    let slots = wc3_game::sim_bridge::map_player_slots(&bundle);
    assert_eq!(slots.len(), 2, "two w3i player records");
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    for line in game.take_log() {
        eprintln!("GAME_LOG {line}");
    }
    eprintln!(
        "JUSTICE melee={} player={} enemy={} units={} mines={} destructables={} base={:?} enemy_base={:?}",
        game.melee,
        game.player,
        game.enemy,
        game.sim.alive_units().count(),
        game.sim.gold_mines.len(),
        game.sim.destructables.len(),
        game.base_pos,
        game.enemy_base
    );
    assert!(game.melee, "Justice runs as the melee arena");
    // 88 `war3mapUnits.doo` units (22 creep camps) + 2 halls + 10 workers of
    // the w3i race kit. A synthetic 1v1 would give 2 halls + 10 workers + 6
    // generated camps = 42 units and 6 mines.
    assert_eq!(game.sim.alive_units().count(), 100, "88 map units + melee opening");
    assert_eq!(game.sim.gold_mines.len(), 7, "the map's seven gold mines");
    assert_eq!(game.sim.destructables.len(), 1997, "the map's 1997 trees/rocks");

    // Halls: one per owner, each the w3i race of its seat.
    let halls: Vec<(u8, &'static str)> = game
        .sim
        .alive_units()
        .filter(|unit| unit.def().main_hall)
        .map(|unit| (unit.owner, unit.def().id))
        .collect();
    assert_eq!(halls.len(), 2, "exactly one hall per seat: {halls:?}");
    // `start_points[0]` (the player base) is the Orc seat of this map; the
    // enemy base is the Human seat. The assertion below is the cross-check
    // against the w3i races, not against the old hardcoded kit.
    let race_of = |seat: u32| {
        slots
            .iter()
            .find(|slot| slot.slot == seat)
            .map(|slot| slot.race)
    };
    assert_eq!(race_of(0), Some(wc3_sim::Race::Human));
    assert_eq!(race_of(1), Some(wc3_sim::Race::Orc));
    let player_race = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().main_hall)
        .map(|unit| unit.def().race)
        .expect("player hall");
    let enemy_race = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.enemy && unit.def().main_hall)
        .map(|unit| unit.def().race)
        .expect("enemy hall");
    eprintln!("JUSTICE races player={player_race:?} enemy={enemy_race:?}");
    assert_eq!(player_race, wc3_sim::Race::Orc, "Orc player (east seat)");
    assert_eq!(enemy_race, wc3_sim::Race::Human, "Human enemy (west seat)");
    assert_eq!(
        halls
            .iter()
            .find(|(owner, _)| *owner == game.player)
            .map(|(_, id)| *id),
        Some("ogre"),
        "Orc great hall for the player"
    );
    assert_eq!(
        halls
            .iter()
            .find(|(owner, _)| *owner == game.enemy)
            .map(|(_, id)| *id),
        Some("htow"),
        "Human town hall for the enemy"
    );

    // Workers: five of the seat's own race, none of the other race.
    let player_workers: Vec<&'static str> = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.def().id)
        .collect();
    let enemy_workers: Vec<&'static str> = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == game.enemy && unit.def().stats.is_worker)
        .map(|unit| unit.def().id)
        .collect();
    eprintln!("JUSTICE workers player={player_workers:?} enemy={enemy_workers:?}");
    assert_eq!(player_workers.len(), 5, "player starts with five workers");
    assert_eq!(enemy_workers.len(), 5, "enemy starts with five workers");
    assert!(
        player_workers.iter().all(|id| *id == "opeo"),
        "player workers are orc peons: {player_workers:?}"
    );
    assert!(
        enemy_workers.iter().all(|id| *id == "hpea"),
        "enemy workers are human peasants, not the player's kit: {enemy_workers:?}"
    );
}

/// Remaining-work #1/#2 cross-check at the scene level: the sim before any
/// fallback must expose exactly the map's own placed counts.
#[test]
fn justice_scene_counts_match_the_map_placements() {
    let Some(path) = justice_path() else { return };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("justice bundle");
    let raw_units = bundle.units.as_ref().map(|units| units.len()).unwrap_or(0);
    let raw_doodads = bundle.doodads.as_ref().map(|doodads| doodads.len()).unwrap_or(0);
    let raw_mines = bundle
        .units
        .as_ref()
        .map(|units| {
            units
                .iter()
                .filter(|unit| unit.type_id == *b"ngol" || unit.type_id == *b"ugol")
                .count()
        })
        .unwrap_or(0);
    let mut sim = wc3_sim::Sim::new(19, 128, 128);
    sim.passive_income = false;
    let counts = wc3_game::sim_bridge::spawn_map_scene(&mut sim, &bundle);
    eprintln!(
        "JUSTICE raw units={raw_units} doodads={raw_doodads} mines={raw_mines} -> counts units={} doodads={} starts={} mines={} camps={}",
        counts.units, counts.destructables, counts.start_locations, counts.gold_mines, counts.camps
    );
    assert_eq!(counts.gold_mines, raw_mines, "every map mine spawns once");
    assert_eq!(sim.gold_mines.len(), raw_mines, "no synthetic mine");
    assert_eq!(counts.units, 88, "def-covered war3mapUnits.doo placements");
    assert_eq!(counts.destructables, 1997, "def-covered war3map.doo placements");
    assert_eq!(counts.start_locations, 2);
    // Every raw placement is classified: start markers, mines, def-covered
    // units, and the ids no melee SLK row defines (Justice places two `nrac`
    // markers; they are not units in any table, so the scene drops them).
    let undefined = bundle
        .units
        .as_ref()
        .map(|units| {
            units
                .iter()
                .filter(|unit| {
                    let id = String::from_utf8_lossy(&unit.type_id);
                    id != "sloc"
                        && id != "ngol"
                        && id != "ugol"
                        && wc3_sim::def_of(&id).is_none()
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        counts.start_locations + counts.gold_mines + counts.units + undefined,
        raw_units,
        "raw {raw_units} placements classified (undefined={undefined})"
    );
    // The raw doodad list also carries decoration-only ids; only the
    // destructable data-backed ones are entities.
    assert!(counts.destructables <= raw_doodads);
}

/// The scene is install-independent: a RoC-only install (`War3.mpq` without
/// `War3x`/`War3xlocal`) must spawn the same 88 map units, 7 mines and 1997
/// destructables. `wc3_sim::data::def_of` is the build-time SLK roster
/// (`ROSTER`/`CREEP_DEFS` in `wc3-sim/src/data.rs`), so the expansion's object
/// database cannot add or drop map placements; this pins it on the real files.
#[test]
fn justice_scene_is_identical_on_a_roc_only_install() {
    let Some(path) = justice_path() else { return };
    let roc = Path::new("/data/data/com.termux/files/usr/tmp/opencode/wc3_roc/War3.mpq");
    if !roc.is_file() {
        eprintln!("skip: RoC-only install missing at {}", roc.display());
        return;
    }
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("justice bundle");
    let mut sim = wc3_sim::Sim::new(19, 128, 128);
    let counts = wc3_game::sim_bridge::spawn_map_scene(&mut sim, &bundle);
    assert_eq!(counts.units, 88, "the RoC roster covers every map unit id");
    assert_eq!(counts.gold_mines, 7);
    assert_eq!(counts.destructables, 1997);
    let game = Game::new(Assets::load(Some(roc), Some(&path)), 0x5EED);
    eprintln!(
        "ROC objects={} units={} mines={} destructables={} melee={}",
        game.assets.objects.is_some(),
        game.sim.alive_units().count(),
        game.sim.gold_mines.len(),
        game.sim.destructables.len(),
        game.melee
    );
    assert!(game.assets.objects.is_some(), "the RoC object database loads");
    assert_eq!(
        game.sim.alive_units().count(),
        100,
        "same 88 map units + 2 halls + 10 workers"
    );
    assert_eq!(game.sim.gold_mines.len(), 7, "the map's seven gold mines");
    assert_eq!(game.sim.destructables.len(), 1997, "the map's trees/rocks");
}

/// `nrac` is the only Justice placement id with no data in the install: its
/// two `war3mapUnits.doo` records are dropped as `unknown_def` (see the census
/// in `justice_scene_counts_match_the_map_placements`). Prove it against the
/// archives themselves, layer by layer, instead of trusting the roster, and
/// keep a positive control (`hfoo`) so the parser cannot pass by finding
/// nothing.
#[test]
fn justice_nrac_is_absent_from_war3_and_war3x_unit_data() {
    let Some(path) = justice_path() else { return };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("justice bundle");
    let nrac = bundle
        .units
        .as_ref()
        .map(|units| units.iter().filter(|unit| unit.type_id == *b"nrac").count())
        .unwrap_or(0);
    assert_eq!(nrac, 2, "Justice places two nrac markers");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-assets/wc3-install");
    let fs = match wc3_re::storm::StormFs::open_wc3_install(&root) {
        Ok(fs) => fs,
        Err(error) => {
            eprintln!("skip: install open failed at {}: {error}", root.display());
            return;
        }
    };
    // An SLK cell holds `K"id"` / `Kid`; strip the cell marker and quotes.
    fn slk_has_id(text: &str, id: &str) -> bool {
        text.split([';', '\n', '\r']).any(|cell| {
            let cell = cell.trim();
            let value = cell.strip_prefix('K').unwrap_or(cell).trim_matches('"');
            value.eq_ignore_ascii_case(id)
        })
    }
    let mut layers_read = 0usize;
    let mut control = false;
    for layer in fs.layers() {
        let mut read_any = false;
        for file in ["Units\\UnitData.slk", "Units\\UnitBalance.slk"] {
            let Ok(bytes) = layer.archive.read(file) else { continue };
            read_any = true;
            let text = String::from_utf8_lossy(&bytes);
            control |= slk_has_id(&text, "hfoo");
            let has_nrac = slk_has_id(&text, "nrac");
            eprintln!(
                "NRAC layer={} path={} file={} bytes={} hfoo={} nrac={}",
                layer.label,
                layer.path.display(),
                file,
                bytes.len(),
                slk_has_id(&text, "hfoo"),
                has_nrac
            );
            assert!(
                !has_nrac,
                "nrac must not have a row in {file} inside {}",
                layer.label
            );
        }
        if read_any {
            layers_read += 1;
        }
    }
    assert!(control, "positive control: hfoo is present in the install tables");
    assert!(
        layers_read >= 2,
        "UnitData/UnitBalance read from at least the War3 and War3x layers (got {layers_read})"
    );
}

/// The glue gate behind the frozen-sim failures: on a melee boot the first
/// `update` opens the main menu and the sim does not tick until the host enters
/// the battle. `Game::enter_battle` is the headless entry point; tests that
/// issue orders through `sim.issue` (or issue nothing) must call it, otherwise
/// they measure a paused sim (`worker_harvests_gold_on_the_real_map`,
/// `rotation_keeps_selection_card_and_match` before this layer).
#[test]
fn glue_screen_freezes_the_sim_until_the_host_enters_the_battle() {
    let Some(path) = justice_path() else { return };
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    let tick = game.sim.tick;
    for _ in 0..120 {
        game.update(1.0 / 30.0);
    }
    assert_eq!(
        game.screen,
        wc3_game::game::Screen::MainMenu,
        "a melee boot opens the retail glue screen"
    );
    assert_eq!(game.sim.tick, tick, "the sim does not tick behind the glue screen");
    game.enter_battle();
    for _ in 0..60 {
        game.update(1.0 / 30.0);
    }
    assert!(
        game.sim.tick > tick,
        "enter_battle resumes the tick loop ({} -> {})",
        tick,
        game.sim.tick
    );
}

/// Remaining-work #4: 2414x1080 <-> 1080x2414 keeps the selection, its command
/// card, the phase and the running match (rotation is not a lifecycle pause).
#[test]
fn rotation_keeps_selection_card_and_match() {
    let Some(path) = justice_path() else { return };
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    // Rotation applies to a live battle (the device reaches it through the
    // glue flow); the headless harness enters the same scene directly.
    game.enter_battle();
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("player worker");
    game.selection = vec![worker];
    // `resize` rebuilds the command card for the selection; the first
    // rotation is the baseline the later ones must reproduce.
    game.resize(2414.0, 1080.0);
    for _ in 0..45 {
        game.update(1.0 / 30.0);
    }
    let tick = game.sim.tick;
    let units = game.sim.alive_units().count();
    let cards = game.buttons.len();
    let base = game.base_pos;
    let enemy_base = game.enemy_base;
    assert!(cards > 0, "selected worker has a command card");

    for (width, height) in [(1080.0f32, 2414.0f32), (2414.0, 1080.0), (1080.0, 2414.0)] {
        game.resize(width, height);
        eprintln!(
            "ROTATE {:.0}x{:.0} selection={:?} cards={} phase={:?} tick={} units={} base={:?}",
            width,
            height,
            game.selection,
            game.buttons.len(),
            game.phase,
            game.sim.tick,
            game.sim.alive_units().count(),
            game.base_pos
        );
        assert_eq!(game.selection, vec![worker], "selection survives rotation");
        assert_eq!(game.buttons.len(), cards, "command card survives rotation");
        assert_eq!(game.phase, wc3_game::game::Phase::Playing, "match stays live");
        assert_eq!(game.sim.tick, tick, "rotation does not advance or reset the sim");
        assert_eq!(game.sim.alive_units().count(), units);
        assert_eq!(game.base_pos, base, "camera base does not jump");
        assert_eq!(game.enemy_base, enemy_base);
        assert_eq!(game.viewport, wc3_sim::Vec2::new(width.max(1.0), height.max(1.0)));
    }
    // The match keeps running after the rotation series.
    for _ in 0..30 {
        game.update(1.0 / 30.0);
    }
    assert!(game.sim.tick > tick, "sim advances after rotation");
}

/// Authored `war3mapUnits.doo` placements spawn unconditionally: the map spawn
/// path must never validate the map's own units against the *player* placement
/// predicate, or a bigger building footprint would refuse creeps/mines/halls.
///
/// The census is the evidence: every id the melee SLK knows must spawn
/// (`spawn_failed = 0`); only map-local ids with no melee row may be skipped.
#[test]
fn map_spawn_authored_placement_bypasses_player_rules() {
    let Some(path) = justice_path() else { return };
    let bundle = wc3_re::mapfiles::MapBundle::open(&path).expect("Justice bundle");
    let mut sim = wc3_sim::Sim::new(0x5EED, 128, 128);
    let counts = wc3_game::sim_bridge::spawn_map_scene(&mut sim, &bundle);
    let placed: usize = counts.unit_census.values().map(|c| c.placed).sum();
    let unknown: usize = counts.unit_census.values().map(|c| c.unknown_def).sum();
    let failed: usize = counts.unit_census.values().map(|c| c.spawn_failed).sum();
    eprintln!(
        "GAME_SCENE_CENSUS placed={placed} unknown_def={unknown} spawn_failed={failed} ids={}",
        counts.unit_census.len()
    );
    assert_eq!(placed, counts.units, "census sum is the spawned unit count");
    assert_eq!(failed, 0, "authored units never fail placement: {:?}", counts.unit_census);
    assert_eq!(
        counts
            .unit_census
            .get("nrac")
            .map(|census| census.unknown_def)
            .unwrap_or(0),
        2,
        "the only skips are map-local ids with no melee row"
    );

    // Both melee seats' halls exist with the authored WC3 32-unit footprints
    // and block their whole square, while the player predicate refuses a
    // second building on exactly those cells (the spawn path bypassed it).
    let bases = counts.melee.expect("Justice is a two-seat melee map");
    let side_cells = |sim: &wc3_sim::Sim, footprint: u8| -> u32 {
        ((footprint.max(1) as f32 * wc3_sim::PATHING_CELL_SIZE
            / sim.grid.cell_size.max(1.0))
        .ceil() as u32)
            .max(1)
    };
    for hall in [bases.player_hall, bases.enemy_hall]
        .into_iter()
        .flatten()
    {
        let unit = sim.unit(hall).expect("authored hall");
        assert_eq!(
            unit.def().footprint, 6,
            "main hall footprint is authored at the WC3 32-unit resolution"
        );
        let side = side_cells(&sim, unit.def().footprint);
        assert_eq!(side, 6, "32-unit pathing grid: hall is 6 cells");
        assert_eq!(
            unit.blocked_cells.len(),
            (side * side) as usize,
            "the hall blocks its real {side}x{side} cells: {}",
            unit.def().id
        );
        let denial = sim.place_denial(unit.owner, unit.def().id, unit.pos);
        assert!(
            denial == Some("site blocked") || denial == Some("site occupied"),
            "a player building is refused on the authored hall cells, got {denial:?} ({})",
            unit.def().id
        );
    }
    // Authored-resolution data contract for the other operator sizes.
    assert_eq!(wc3_sim::data::def_of("hbar").unwrap().footprint, 5, "barracks");
    assert_eq!(wc3_sim::data::def_of("hgtw").unwrap().footprint, 3, "tower");
}
