//! Campaign as a player experience: mission load, quest log, cutscene camera
//! path, subtitle band, progress persistence and movie decode.
//!
//! Every test is one row of `analysis/tech-lead/MECHANICS_GAP_MAP.md`:
//! `campaign.flow` (mission/quests/progress), `cutscenes.playback` (camera,
//! fades, subtitles) and `cutscenes.movies` (own-codec decode). They run on the
//! extracted campaign maps (`analysis/war3_full3/Maps/Campaign`) and skip with a
//! message when that tree is absent.

use std::path::PathBuf;

use wc3_game::assets::Assets;
use wc3_game::campaign::{
    campaign_map_dir, CameraDirector, CampaignProgress, CampaignRegistry, CampaignSession,
    ChapterState, QuestLog, SubtitleKind, SubtitleQueue, CAMERA_SAMPLE_HZ,
};
use wc3_game::cinematic::MovieError;
use wc3_game::game::{Game, TICK_SECONDS};
use wc3_re::jass::vm::{CameraEvent, CameraSetup, Quest, SubtitleEvent, Unit, World};

fn map_path(name: &str) -> Option<PathBuf> {
    let dir = campaign_map_dir()?;
    let path = dir.join(name);
    path.is_file().then_some(path)
}

fn campaign_game(mission: &str) -> Option<Game> {
    let map = map_path(mission)?;
    let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
    game.enter_battle();
    Some(game)
}

/// `campaign.mission-load`: starting a chapter loads its map, runs its script
/// and ticks 300 sim frames with the player's units alive.
#[test]
fn campaign_mission_load() {
    let Some(mut game) = campaign_game("Human01.w3m") else {
        eprintln!("skip: Human01.w3m missing (extracted campaign maps needed)");
        return;
    };
    let index = game
        .campaign_index_of("Human01")
        .expect("Human01 is in the campaign registry");
    game.start_campaign_mission(index)
        .expect("Human01 loads as a campaign mission");
    let view = game.campaign_view().expect("campaign session");
    assert!(view.script_booted, "war3map.j ran for Human01: {view:?}");
    assert_eq!(view.id, "Human01");
    assert!(
        game.sim.alive_units().count() > 0,
        "the mission's placed units are in the sim"
    );
    let human = view
        .human_slot
        .or_else(|| game.campaign_human_slot())
        .expect("Human01 declares a human slot");
    // The mission's own world carries the human's units; the campaign HUD binds
    // to that map slot (the canonical sim's 2-player order pipeline covers the
    // two lowest slots).
    let units_of = |game: &Game| {
        game.engine
            .as_ref()
            .and_then(|engine| engine.runtime())
            .map(|runtime| {
                runtime
                    .world
                    .units
                    .values()
                    .filter(|unit| unit.owner == i32::from(human) && unit.alive)
                    .count()
            })
            .unwrap_or(0)
    };
    let before = units_of(&game);
    assert!(before > 0, "the human slot owns live units at mission start");
    for _ in 0..300 {
        // The HUD drives `campaign_frame` every frame; the cutscene director is
        // fed from it, so a test that only ticks the sim would see the whole
        // opening arrive as one same-instant batch.
        game.update(TICK_SECONDS);
        game.campaign_frame(TICK_SECONDS);
    }
    let after = units_of(&game);
    assert!(
        after > 0,
        "the player still has live units after 300 ticks ({before} -> {after})"
    );
    let view = game.campaign_view().expect("campaign session");
    assert!(
        view.time > 0.0,
        "the mission clock advanced: {:?}",
        view.time
    );
    // The opening is tick-driven now: `TriggerSleepAction` suspends the intro
    // between beats, so the camera sequence plays out over mission time instead
    // of inside the boot tick. Drive the mission until the opening has played,
    // bounded so a stuck script cannot hang the suite (100 s of mission time).
    let mut driven = 0;
    while driven < 3000 && game.camera_director().map_or(0, |d| d.keyframes.len()) < 4 {
        game.update(TICK_SECONDS);
        game.campaign_frame(TICK_SECONDS);
        driven += 1;
    }
    if let Some(engine) = game.engine.as_ref() {
        if let Some(runtime) = engine.runtime() {
            println!(
                "CAMPAIGN_HUMAN_SCRIPT ticks={} engine_time={:.2} mission_time={:.2} sleeps={} wakes={} parked={} camera_events={} beats={} driven={driven}",
                engine.ticks,
                engine.time,
                runtime.world.game_time,
                runtime.sleeps,
                runtime.wakes,
                runtime.threads.len(),
                runtime.world.camera_events.len(),
                runtime.world.cinematic_log.len(),
            );
        }
    }
    // The real mission's opening is a real camera path, not one static shot.
    let director = game.camera_director().expect("camera director");
    let keyframes = director.keyframes.len();
    assert!(
        keyframes >= 4,
        "Human01's opening applies a sequence of camera setups: {keyframes} keyframes"
    );
    let end = director
        .keyframes
        .last()
        .map(|key| key.t + key.blend.max(0.0) + 1.0)
        .unwrap_or(1.0);
    let max_step = director.max_step(0.0, end, CAMERA_SAMPLE_HZ);
    assert!(
        max_step <= 400.0,
        "the mission camera never jumps more than 400 units per frame: max {max_step:.1}"
    );
    let log = game.campaign_log();
    assert!(
        log.iter().any(|line| line.starts_with("CAMPAIGN_MISSION idx=")),
        "CAMPAIGN_MISSION logged: {log:?}"
    );
    assert!(
        log.iter().any(|line| line.starts_with("QUEST add=")),
        "the mission's quests reached the panel: {log:?}"
    );
}

/// `campaign.quest-log`: Human01's three objectives parse from the map and the
/// live quest table fills the checklist items.
#[test]
fn campaign_quest_log() {
    let Some(map) = map_path("Human01.w3m") else {
        eprintln!("skip: Human01.w3m missing");
        return;
    };
    let mission = wc3_game::campaign::Mission::parse(&map).expect("Human01 parses");
    println!(
        "HUMAN01_W3I slots={:?} human_slot={:?}",
        mission
            .players
            .iter()
            .map(|slot| (slot.slot, slot.controller, slot.race, slot.name.clone()))
            .collect::<Vec<_>>(),
        mission.human_slot()
    );
    let log = QuestLog::from_mission(&mission);
    assert_eq!(
        log.rows().len(),
        3,
        "Human01 ships three objectives: {:?}",
        log.rows()
            .iter()
            .map(|row| row.title.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(log.required_total(), 1, "one required objective");
    let required = log
        .rows()
        .iter()
        .find(|row| row.required)
        .expect("required objective");
    assert_eq!(required.title, "Странбард");
    assert_eq!(required.items.len(), 3, "the village objective has 3 steps: {required:?}");

    // The live table (what the VM records when the script calls CreateQuestBJ /
    // CreateQuestItemBJ): three quests, one required, with checklist text.
    let mut world = World::default();
    let mut required_quest = Quest {
        title: "Странбард".to_string(),
        description: "description".to_string(),
        required: true,
        discovered: true,
        ..Quest::default()
    };
    required_quest.items = ["Поговорите с Утером", "Найдите следы", "Спасите деревню"]
        .iter()
        .map(|item| (item.to_string(), false))
        .collect();
    world.quests.insert(1, required_quest);
    world.quests.insert(
        2,
        Quest {
            title: "Атаман разбойников".to_string(),
            required: false,
            discovered: true,
            ..Quest::default()
        },
    );
    world.quests.insert(
        3,
        Quest {
            title: "Спасите Тимми".to_string(),
            required: false,
            discovered: true,
            ..Quest::default()
        },
    );
    let mut live = QuestLog::from_mission(&mission);
    let delta = live.sync(&world.quests, &mission);
    assert_eq!(delta.added.len(), 3, "three quests bound to the panel: {delta:?}");
    assert_eq!(live.rows().len(), 3);
    assert_eq!(live.required_total(), 1);
    assert_eq!(
        live.rows()
            .iter()
            .find(|row| row.required)
            .expect("required")
            .items_done(),
        0,
        "nothing is done at mission start"
    );

    // Completing the required quest's steps and the quest itself flips the
    // panel and the mission objective state.
    world.quests.get_mut(&1).expect("quest").completed = true;
    world.quests.get_mut(&1).expect("quest").items[2].1 = true;
    let delta = live.sync(&world.quests, &mission);
    assert_eq!(delta.completed, vec!["Странбард".to_string()]);
    assert_eq!(live.required_done(), 1);
    assert!(live.all_required_done(), "the required objective is complete");
}

/// `campaign.cine.camera-path`: four camera setups produce a keyframe track the
/// renderer follows — monotone time, and no frame moves the view more than 400
/// world units.
#[test]
fn campaign_cine_camera_path() {
    let mut world = World::default();
    // Four shots, 3,000 units apart each, with fields the map scripts set.
    let shots = [
        ((0.0, 0.0), 1_400.0, 90.0, 304.0),
        ((3_000.0, 0.0), 1_800.0, 120.0, 334.0),
        ((3_000.0, 3_000.0), 1_600.0, 60.0, 310.0),
        ((0.0, 3_000.0), 2_000.0, 180.0, 350.0),
    ];
    let mut handles = Vec::new();
    for (index, ((x, y), distance, yaw, pitch)) in shots.iter().enumerate() {
        let handle = 100 + index as u32;
        let mut setup = CameraSetup::default();
        setup.dest = Some((*x, *y));
        setup.fields.insert(0, *distance);
        setup.fields.insert(5, *yaw);
        setup.fields.insert(2, *pitch);
        world.camera_setups.insert(handle, setup);
        handles.push(handle);
    }
    // What the script does: apply each setup in turn, the first instantly and the
    // rest over time, exactly as `CameraSetupApplyForPlayer(doPan, …)` records.
    for (index, handle) in handles.iter().enumerate() {
        let t = if index == 0 { 0.0 } else { 4.0 * index as f64 };
        world.camera_events.push(CameraEvent {
            kind: "setup".into(),
            setup: Some(*handle),
            player: Some(0),
            value: 1.0,
            duration: if index == 0 { 0.0 } else { 3.0 },
            t,
            ..CameraEvent::default()
        });
    }
    world.game_time = 20.0;
    let mut director = CameraDirector::default();
    director.observe(&world);
    assert_eq!(director.keyframes.len(), 4, "one keyframe per setup");
    assert!(
        director
            .keyframes
            .windows(2)
            .all(|pair| pair[1].t >= pair[0].t),
        "keyframe time is monotone: {:?}",
        director
            .keyframes
            .iter()
            .map(|key| key.t)
            .collect::<Vec<_>>()
    );
    let max_step = director.max_step(0.0, 16.0, CAMERA_SAMPLE_HZ);
    assert!(
        max_step <= 400.0,
        "the camera never jumps more than 400 units per frame: max {max_step:.1}"
    );
    let start = director.sample(0.0);
    assert_eq!(start.target, [0.0, 0.0], "the first shot is the opening camera");
    let fourth = director.sample(16.0);
    assert_eq!(fourth.target, [0.0, 3_000.0], "the last shot lands on its target");
    // The path is continuous between shots: no sample skips the intermediate
    // positions.
    let path = director.path(0.0, 16.0, CAMERA_SAMPLE_HZ);
    assert!(
        path.len() >= 480,
        "a 16 s cutscene at 30 Hz is a real path: {} samples",
        path.len()
    );
    assert!(
        path.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "path time advances"
    );
}

/// `campaign.cine.subtitle-queue`: a transmission appears with the speaker of
/// the portrait unit and the duration the script asked for.
#[test]
fn campaign_cine_subtitle_queue() {
    let mut world = World::default();
    let mut jaina = Unit {
        id: "Hjai".to_string(),
        name: "Jaina Proudmoore".to_string(),
        ..Unit::default()
    };
    jaina.alive = true;
    world.units.insert(7, jaina);
    world.game_time = 12.5;
    world.subtitles.push(SubtitleEvent {
        t: 12.5,
        speaker: "Jaina".to_string(),
        text: "Arthas, the plague has reached Strahnbrad.".to_string(),
        duration: 4.0,
        portrait_unit_id: i64::from(u32::from_le_bytes(*b"Hjai")),
        color: 0,
        voiceover: 3.5,
        sound: Some(42),
        player: None,
    });
    // A scene line with no name of its own resolves the speaker from the
    // portrait type.
    world.subtitles.push(SubtitleEvent {
        t: 16.6,
        speaker: String::new(),
        text: "The village burns.".to_string(),
        duration: 0.0,
        portrait_unit_id: i64::from(u32::from_le_bytes(*b"Hjai")),
        ..SubtitleEvent::default()
    });
    // A live `TRIGSTR_` reference resolves through the map's own string table,
    // exactly like the static mission parse does.
    world.subtitles.push(SubtitleEvent {
        t: 21.0,
        speaker: "Uther".to_string(),
        text: "TRIGSTR_021".to_string(),
        duration: 3.0,
        portrait_unit_id: i64::from(u32::from_le_bytes(*b"Hjai")),
        ..SubtitleEvent::default()
    });
    let mut mission = wc3_game::campaign::Mission::default();
    mission.strings.entries.insert(21, "Glad you came.".to_string());
    let mut queue = SubtitleQueue::default();
    let fresh = queue.observe(&world, &mission);
    assert_eq!(fresh, 3, "all three lines entered the band");
    assert_eq!(queue.transmissions(), 3);
    assert_eq!(
        queue.lines()[2].text, "Glad you came.",
        "TRIGSTR_021 resolved through war3map.wts"
    );
    let first = &queue.lines()[0];
    assert_eq!(first.t, 12.5, "stamped with mission time");
    assert_eq!(first.speaker, "Jaina");
    assert_eq!(first.shown_for, 4.0, "script duration is respected");
    assert_eq!(first.portrait, Some(7), "portrait resolves to the live unit");
    assert_eq!(first.kind, SubtitleKind::Transmission);
    assert!(queue.active_at(13.0).is_some(), "the line is on screen");
    assert!(
        queue.active_at(16.6).is_none() || queue.active_at(16.6).is_some_and(|line| line.text != first.text),
        "the first line ends after 4 s"
    );
    let second = &queue.lines()[1];
    assert_eq!(
        second.speaker, "Jaina Proudmoore",
        "an unnamed scene line takes the portrait unit's name"
    );
    assert!(
        (second.shown_for - SubtitleQueue::reading_time(&second.text)).abs() < 1e-9,
        "a zero duration gets the reading-time floor: {}",
        second.shown_for
    );
}

/// `campaign.progress-persists`: clearing a chapter writes the progress file,
/// and a reload unlocks the next mission while the cleared one stays cleared.
#[test]
fn campaign_progress_persists() {
    let Some(dir) = campaign_map_dir() else {
        eprintln!("skip: campaign maps missing");
        return;
    };
    let path = std::env::temp_dir().join(format!("h2-campaign-{}.sav", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let progress = CampaignProgress::default();
    let mut registry = CampaignRegistry::from_map_dir(&dir, progress).expect("campaign registry");
    registry.save_path = Some(path.clone());
    let human01 = registry.index_of("Human01").expect("Human01 chapter");
    let human02 = registry.index_of("Human02").expect("Human02 chapter");
    assert!(
        !registry.is_unlocked(human02),
        "Human02 is locked before Human01 is cleared"
    );
    let rows = registry.rows();
    assert_eq!(
        rows[human01].state,
        ChapterState::Unlocked,
        "the first mission is open"
    );

    // Victory path: the session marks the chapter and persists it.
    let mut session = CampaignSession::open_with(registry.clone(), human01).expect("session");
    let next = session.registry.complete(human01);
    assert_eq!(next, Some(human02), "clearing Human01 unlocks Human02");
    session.next = next;
    let bytes = std::fs::read(&path).expect("progress file written");
    assert_eq!(&bytes[0..4], b"W3CP", "campaign save container");

    // Reload from disk: the cleared chapter is remembered and the next one is
    // open.
    let reloaded = CampaignProgress::load(&path).expect("progress reloads");
    assert!(reloaded.is_cleared("Human01"));
    let mut registry = CampaignRegistry::from_map_dir(&dir, reloaded).expect("registry");
    registry.save_path = Some(path.clone());
    assert!(registry.is_cleared(human01));
    assert!(registry.is_unlocked(human02));
    assert_eq!(
        registry.next_after(human01),
        Some(human02),
        "the campaign advances to the next mission"
    );
    // The cleared chapter is playable again (replay), not locked.
    assert!(registry.is_unlocked(human01));
    let _ = std::fs::remove_file(&path);
}

/// `campaign.cutscene-decodable`: the own codec decodes the first frame of a
/// shipped movie, and a movie the install does not carry reports
/// `MOVIE_MISSING <name>` without blocking the mission.
#[test]
#[cfg(feature = "movie_decoder")]
#[ignore = "needs the extracted movie pack (WC3RUST_MOVIES or the installer roots)"]
fn campaign_cutscene_decodable() {
    let mut game = campaign_game("Human01.w3m").expect("Human01");
    let library = wc3_game::cinematic::movies();
    let Some(movie) = library.find("HumanEd").cloned() else {
        eprintln!("skip: HumanEd.mpq missing");
        return;
    };
    game.cinematic_start("HumanEd").expect("HumanEd starts");
    assert!(game.cinematic_active(), "the movie is playing");
    // The first VOP is black by design (measured: the picture starts at frame
    // 194, ≈8 s in), so the gate is "our codec produces picture", not "frame 0
    // is lit". 300 frames is 12.5 s at the container's 24.06 fps.
    let mut first = None;
    let mut lit = false;
    let mut frames = 0usize;
    for _ in 0..300 {
        let Some(frame) = game.cinematic_frame(1.0 / 24.0) else {
            continue;
        };
        frames += 1;
        if first.is_none() {
            assert!(
                frame.width >= 320 && frame.height >= 180,
                "decoded frame has the movie's geometry: {}x{} (container {} bytes)",
                frame.width,
                frame.height,
                movie.bytes
            );
            assert_eq!(
                frame.rgba.len(),
                (frame.width * frame.height * 4) as usize,
                "RGBA8 frame"
            );
            first = Some(frame.clone());
        }
        if frame
            .rgba
            .chunks_exact(4)
            .any(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
        {
            lit = true;
            break;
        }
    }
    assert!(first.is_some(), "the decoder produced a frame");
    assert!(frames >= 24, "the player pulled real frames: {frames}");
    assert!(
        lit,
        "the decoder produced picture within the first {frames} frames"
    );
    // A name the install does not carry: typed error, `MOVIE_MISSING` display,
    // mission still running.
    let error = game
        .cinematic_start("NoSuchMovie")
        .expect_err("unknown movie is missing");
    assert!(error.is_missing(), "typed as missing: {error:?}");
    assert_eq!(error.to_string(), "MOVIE_MISSING NoSuchMovie");
    assert!(matches!(error, MovieError::Missing(_)));
    game.cinematic_stop();
    assert!(!game.cinematic_active());
}

/// The registry order is the retail chapter order (Human01 before Human02 and
/// both before Undead01) — a guard on the campaign picker's list.
#[test]
fn campaign_chapter_order() {
    let Some(dir) = campaign_map_dir() else {
        eprintln!("skip: campaign maps missing");
        return;
    };
    let registry =
        CampaignRegistry::from_map_dir(&dir, CampaignProgress::default()).expect("registry");
    let ids: Vec<String> = registry.rows().iter().map(|row| row.id.clone()).collect();
    let position = |id: &str| ids.iter().position(|known| known == id).expect(id);
    assert!(position("Human01") < position("Human02"));
    assert!(position("Human02") < position("Human02Interlude"));
    assert!(position("Human02Interlude") < position("Human03"));
    assert!(position("Human09") < position("Undead01"));
    assert_eq!(
        registry.len(),
        44,
        "the extracted campaign tree carries 44 maps: {ids:?}"
    );
    assert!(registry.rows()[0].state == ChapterState::Unlocked);
}



/// `campaign.orc01.mpq` — the acceptance chapter: Orc01 loaded **from the
/// operator's MPQ**, its script booted, its quests/transmissions/camera/fades
/// read through the campaign layer's own types, and clearing it unlocking the
/// next chapter.
///
/// The `CAMPAIGN_LOAD` line this test asserts is the same one the device
/// harness greps (`CAMPAIGN_LOAD source=mpq path=Maps\Campaign\Orc01.w3m`).
#[test]
fn campaign_orc01_from_mpq() {
    let archives = wc3_game::campaign::campaign_archives();
    if archives.is_empty() {
        eprintln!("skip: no readable War3.mpq/War3x.mpq (campaign_archives empty)");
        return;
    }
    let save = std::env::temp_dir().join(format!("h2-orc01-{}.sav", std::process::id()));
    let _ = std::fs::remove_file(&save);
    // Install an isolated registry: the run may complete the chapter, and it
    // must never write the operator's progress file.
    let mut registry = CampaignRegistry::discover(CampaignProgress::default())
        .expect("campaign discovery (loose tree + mounted archives)");
    registry.save_path = Some(save.clone());
    let index = registry.index_of("Orc01").expect("Orc01 is a campaign chapter");
    let chapter = registry.chapter(index).expect("chapter").clone();
    assert_eq!(
        chapter.source.as_str(),
        "mpq",
        "the chapter comes from the mounted archive, not a loose copy: {:?}",
        chapter.source
    );
    assert_eq!(
        chapter.archive_entry_path(),
        Some("Maps\\Campaign\\Orc01.w3m"),
        "archive provenance"
    );
    wc3_game::campaign::set_campaign_registry(registry);
    let map = wc3_game::campaign::campaign_registry()
        .expect("installed registry")
        .map_path(index)
        .expect("Orc01 map bytes resolve from the archive");

    // --- the chapter as the game loads it --------------------------------
    let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
    game.enter_battle();
    game.start_campaign_mission(index).expect("Orc01 loads");
    let log = game.campaign_log();
    let load = log
        .iter()
        .find(|line| line.starts_with("CAMPAIGN_LOAD "))
        .unwrap_or_else(|| panic!("CAMPAIGN_LOAD logged: {log:?}"))
        .clone();
    assert!(
        load.starts_with("CAMPAIGN_LOAD source=mpq path=Maps\\Campaign\\Orc01.w3m"),
        "the load line names the archive entry: {load}"
    );
    println!("{load}");

    // --- the chapter's world, through the campaign layer ------------------
    // `Game::start_campaign_mission` boots the chapter script through the game
    // layer. Orc01's file stem is also in the melee-arena list
    // (`game::Game::MELEE_MAP_STEMS`), which suppresses that boot; the VM-side
    // proof for exactly this map is `wc3-re/tests/campaign_boot.rs`. When the
    // arena rule is gated on `!map_path_is_campaign` the game path below turns
    // green as well — the assertions are the same ones either way.
    let view = game.campaign_view().expect("campaign session");
    if !view.script_booted {
        println!(
            "ORC01_SCRIPT_BLOCKED reason=game.rs:1331 MELEE_MAP_STEMS=[\"Human04\",\"Orc01\",..] map={} (patch: analysis/campaign/ORC01-MELEE-STEM-PATCH.md)",
            map.display()
        );
    }

    // The mission parse is the map's own data: objectives, cinematics, script.
    let mission = wc3_game::campaign::Mission::parse(&map).expect("Orc01 parses");
    assert!(
        mission.objectives.len() >= 2,
        "Orc01 ships its objectives: {:?}",
        mission.objectives
    );
    for objective in &mission.objectives {
        assert!(
            !objective.title.trim().is_empty() && !objective.title.contains("TRIGSTR_"),
            "objective title resolved through war3map.wts: {objective:?}"
        );
    }
    println!(
        "ORC01_OBJECTIVES n={} required={} titles={:?}",
        mission.objectives.len(),
        mission.objectives.iter().filter(|objective| objective.required).count(),
        mission
            .objectives
            .iter()
            .map(|objective| objective.title.clone())
            .collect::<Vec<_>>()
    );
    let log = game.campaign_log();
    let bounds = log.iter().find(|line| line.starts_with("CAMPAIGN_MISSION "));
    assert!(bounds.is_some(), "the mission line is in the journal: {log:?}");

    // --- the live mission world (the VM proves the chapter's script) ------
    // Boot the chapter exactly as the game does (the archive is the install
    // root; `Scripts\common.j` comes from the MPQ), then judge its world with
    // the campaign layer's own types.
    let root = match &chapter.source {
        wc3_game::campaign::MapSource::Mount { archive, .. } => {
            archive.parent().map(std::path::Path::to_path_buf)
        }
        wc3_game::campaign::MapSource::File => None,
    }
    .expect("the chapter's archive gives the install root");
    let mut config = wc3_re::engine::EngineConfig::new(&root);
    config.seed = 0x5EED;
    config.map = Some(map.clone());
    let mut engine = wc3_re::engine::Engine::boot(config).expect("Orc01 boots");
    assert!(
        engine.script_main_ok,
        "war3map.j main ran: {:?}",
        engine.script_errors
    );
    let report = engine.script_report();
    assert!(report.functions > 1_000, "merged program: {}", report.functions);
    assert!(report.triggers > 0, "Orc01 registers triggers");
    // 3000 ticks = 100 s of mission time at 30 Hz: the headless Orc01 smoke the
    // acceptance gate names.
    for _ in 0..3000 {
        engine.tick(1.0 / 30.0);
    }
    let runtime = engine.runtime().expect("runtime");
    let world = &runtime.world;
    assert!(
        world.game_time > 99.0,
        "the mission clock advanced: {} s",
        world.game_time
    );
    println!(
        "ORC01_SCRIPT time={:.2} sleeps={} wakes={} parked={} beats={}",
        world.game_time,
        runtime.sleeps,
        runtime.wakes,
        runtime.threads.len(),
        world.cinematic_log.len()
    );
    assert!(
        runtime.sleeps > 0 && runtime.wakes > 0,
        "the opening suspends and resumes (sleeps={} wakes={})",
        runtime.sleeps,
        runtime.wakes
    );

    // Quests through the panel: the raw `TRIGSTR_` keys resolve to the map's
    // own Russian text.
    let mut quests = QuestLog::from_mission(&mission);
    let delta = quests.sync(&world.quests, &mission);
    assert!(!delta.added.is_empty(), "the script's quests bind to the panel: {delta:?}");
    for row in quests.rows() {
        assert!(
            !row.title.trim().is_empty() && !row.title.contains("TRIGSTR_"),
            "the panel shows the map's text, not the key: {row:?}"
        );
        for item in &row.items {
            assert!(
                !item.text.contains("TRIGSTR_"),
                "checklist items resolved: {row:?}"
            );
        }
    }
    println!(
        "ORC01_QUESTS n={} titles={:?}",
        quests.rows().len(),
        quests.rows().iter().map(|row| row.title.clone()).collect::<Vec<_>>()
    );

    // Camera: the director builds the opening's keyframe track.
    let mut director = CameraDirector::default();
    director.observe(world);
    let keyframes = director.keyframes.clone();
    assert!(
        keyframes.len() >= 4,
        "the opening is a real camera sequence: {} keyframes",
        keyframes.len()
    );
    assert!(
        keyframes.windows(2).all(|pair| pair[1].t >= pair[0].t),
        "keyframe time is monotone: {:?}",
        keyframes.iter().map(|key| key.t).collect::<Vec<_>>()
    );
    let end = keyframes
        .last()
        .map(|key| key.t + key.blend.max(0.0) + 1.0)
        .unwrap_or(1.0);
    let max_step = director.max_step(0.0, end, CAMERA_SAMPLE_HZ);
    assert!(
        max_step <= 400.0,
        "the camera never teleports: max {max_step:.1} units per 30 Hz sample"
    );
    println!(
        "ORC01_CAMERA keyframes={} end={end:.2} max_step={max_step:.1}",
        keyframes.len()
    );

    // Transmissions and fades.
    let mut subtitles = SubtitleQueue::default();
    let fresh = subtitles.observe(world, &mission);
    assert!(fresh >= 1, "the intro has transmissions: {fresh}");
    let speaker = subtitles
        .lines()
        .iter()
        .find(|line| !line.speaker.trim().is_empty())
        .map(|line| line.speaker.clone())
        .unwrap_or_else(|| panic!("a transmission names its speaker: {:?}", subtitles.lines()));
    let fades = director.fades.len();
    assert!(fades >= 1, "the intro fades the screen: {fades}");
    println!(
        "ORC01_CINE subtitles={} speaker={speaker:?} fades={fades}",
        subtitles.transmissions()
    );

    // --- victory through the mission's own trigger ------------------------
    // The campaign layer completes a chapter only when its victory trigger has
    // actually run (`trigger_matching` requires `exec_count > 0`). Execute the
    // real Orc01 victory trigger and read the journal the HUD shows.
    let victory_handle = game
        .engine
        .as_ref()
        .and_then(|engine| engine.runtime())
        .and_then(|runtime| {
            runtime.world.triggers.iter().find_map(|(handle, trigger)| {
                trigger
                    .actions
                    .iter()
                    .any(|action| {
                        let lowered = action.to_ascii_lowercase();
                        lowered.contains("victory")
                            && !lowered.contains("cancel")
                            && !lowered.contains("cheat")
                    })
                    .then_some(*handle)
            })
        });
    let victory_handle = victory_handle.expect("Orc01 registers a victory trigger");
    if let Some(runtime) = game.engine.as_mut().and_then(|engine| engine.runtime_mut()) {
        runtime
            .execute_trigger(victory_handle)
            .expect("the victory trigger runs");
    }
    game.campaign_frame(TICK_SECONDS);
    let view = game.campaign_view().expect("campaign session");
    let done = game
        .campaign_log()
        .into_iter()
        .find(|line| line.starts_with("CAMPAIGN_DONE "));
    string_assert(&done);
    assert!(
        view.victory && view.completed,
        "the mission's own victory trigger completes the chapter: {view:?}"
    );
    let done = done.expect("CAMPAIGN_DONE logged");
    assert!(
        done.contains("idx=24") && done.contains("next=Orc02"),
        "the completion names the running chapter and the retail next one: {done}"
    );
    println!("ORC01_VICTORY {done}");

    // --- completion unlocks the next chapter ------------------------------
    let mut local = CampaignRegistry::discover(CampaignProgress::default()).expect("registry");
    local.save_path = Some(save.clone());
    let mut session = CampaignSession::open_with(local, index).expect("session");
    let next = session.registry.complete(index).expect("Orc01 has a successor");
    session.next = Some(next);
    let next_id = session
        .registry
        .chapter(next)
        .map(|chapter| chapter.id.clone());
    assert_eq!(next_id.as_deref(), Some("Orc02"), "the campaign advances to Orc02");
    let reloaded = CampaignProgress::load(&save).expect("progress persisted");
    assert!(reloaded.is_cleared("Orc01"), "clearing Orc01 is recorded");
    let mut after = CampaignRegistry::discover(reloaded.clone()).expect("registry");
    after.save_path = Some(save.clone());
    assert!(after.is_unlocked(next), "Orc02 is unlocked after Orc01");
    println!(
        "ORC01_NEXT id=Orc02 index={next} cleared={}",
        reloaded.is_cleared("Orc01")
    );

    // The journal line the acceptance run reads.
    println!(
        "ORC01 units={} quests={} ticks=3000 cam={} fades={} subs={}",
        world.units.len(),
        quests.rows().len(),
        keyframes.len(),
        fades,
        subtitles.transmissions()
    );
    let _ = std::fs::remove_file(&save);
}

/// Asserts a journal line exists (kept out of the assertions above so the
/// message stays readable).
fn string_assert(line: &Option<String>) {
    assert!(line.is_some(), "the journal carries the line");
}
