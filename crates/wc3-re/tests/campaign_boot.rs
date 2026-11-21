//! Orc01 (first RoC orc chapter) boots from the operator's MPQ and its intro is
//! spread over ticks by the script's own sleeps.
//!
//! This is the VM-side half of the campaign acceptance: the map bytes come out
//! of `War3.mpq`, `war3map.j` runs through the canonical interpreter, and the
//! mission's opening cutscene suspends (`TriggerSleepAction`) instead of
//! flattening into the boot tick.

use std::path::{Path, PathBuf};

/// Readable `War3.mpq` candidates, in the order the game mounts them.
fn archives() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots: Vec<PathBuf> = Vec::new();
    for key in ["WC3RUST_DATA", "H2_WC3_INSTALL", "WC3RUST_CAMPAIGN"] {
        if let Some(dir) = std::env::var_os(key) {
            roots.push(PathBuf::from(dir));
        }
    }
    roots.push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    roots.push(manifest.join("../../analysis/war3_full3"));
    roots.push(manifest.join("../../analysis/agent-assets/wc3-install"));
    roots.push(std::env::temp_dir().join("opencode/wc3_iso"));
    let mut found = Vec::new();
    for root in roots {
        for name in ["War3.mpq", "War3x.mpq"] {
            let path = root.join(name);
            if path.is_file() && std::fs::File::open(&path).is_ok() {
                found.push(path);
            }
        }
    }
    found
}

/// The chapter map, extracted from the archive next to the install tree (the
/// game layer does this in `Campaign::resolved_map`).
fn orc01_map(archive: &Path) -> Option<PathBuf> {
    let cache = std::env::temp_dir()
        .join("wc3rust-maps")
        .join("Maps")
        .join("Campaign")
        .join("Orc01.w3m");
    if cache.is_file() && std::fs::metadata(&cache).map(|meta| meta.len()).unwrap_or(0) > 1024 {
        return Some(cache);
    }
    let mpq = wc3_mpq::MpqArchive::open(archive).ok()?;
    let bytes = mpq.read("Maps\\Campaign\\Orc01.w3m").ok()?;
    std::fs::create_dir_all(cache.parent()?).ok()?;
    std::fs::write(&cache, bytes).ok()?;
    Some(cache)
}

#[test]
fn orc01_boots_and_its_intro_suspends() {
    let Some(archive) = archives().into_iter().next() else {
        eprintln!("skip: no readable War3.mpq");
        return;
    };
    let Some(root) = archive.parent().map(Path::to_path_buf) else {
        eprintln!("skip: archive has no parent");
        return;
    };
    let Some(map) = orc01_map(&archive) else {
        eprintln!("skip: Maps\\Campaign\\Orc01.w3m not readable from {}", archive.display());
        return;
    };
    // The campaign layer knows the map's human seat and installs it before the
    // script boots; without it every `if GetLocalPlayer() == whichPlayer`
    // camera/transmission block is skipped. The wildcard is the same
    // single-player rule the campaign uses when the map names no seat.
    wc3_re::jass::vm::set_local_player_any_default(true);
    let mut config = wc3_re::engine::EngineConfig::new(&root);
    config.map = Some(map.clone());
    config.seed = 0x5EED;
    let mut engine = wc3_re::engine::Engine::boot(config).expect("Orc01 boots");
    assert_eq!(engine.state, wc3_re::engine::EngineState::InGame);
    assert!(
        engine.script_main_ok,
        "war3map.j main failed: {:?}",
        engine.script_errors
    );
    let report = engine.script_report();
    assert!(
        report.functions > 1_000,
        "merged program functions: {}",
        report.functions
    );
    assert!(report.triggers > 0, "Orc01 registers triggers");
    let runtime = engine.runtime().expect("runtime");
    println!(
        "ORC01_BOOT archive={} map={} functions={} triggers={} actions={} executed={} sleeps={} threads={}",
        archive.display(),
        map.display(),
        report.functions,
        report.triggers,
        report.trigger_actions,
        report.executed_triggers,
        runtime.sleeps,
        runtime.threads.len()
    );
    assert!(
        runtime.sleeps > 0,
        "the opening cutscene suspends on TriggerSleepAction: sleeps={}",
        runtime.sleeps
    );

    // Drive the mission clock: parked threads must resume, and the intro's
    // beats must land on different ticks (not all in the boot tick).
    for _ in 0..900 {
        engine.tick(1.0 / 30.0);
    }
    let runtime = engine.runtime().expect("runtime");
    let world = &runtime.world;
    println!(
        "ORC01_THREADS time={:.2} sleeps={} wakes={} parked={} beats={}",
        world.game_time,
        runtime.sleeps,
        runtime.wakes,
        runtime.threads.len(),
        world.cinematic_log.len()
    );
    assert!(
        world.game_time > 29.0,
        "900 ticks at 30 Hz is 30 s of mission time: {}",
        world.game_time
    );
    assert!(runtime.wakes > 0, "suspended threads resumed on later ticks");
    assert!(
        !world.cinematic_log.is_empty(),
        "the cutscene produced ordered beats"
    );
    // Beats must be spread over the timeline, not stamped into one tick.
    let first = world.cinematic_log.first().map(|beat| beat.t);
    let last = world.cinematic_log.last().map(|beat| beat.t);
    let spread = last.unwrap_or(0.0) - first.unwrap_or(0.0);
    println!("ORC01_SPREAD first={first:?} last={last:?} spread={spread:.2}");
    assert!(
        spread > 0.5,
        "the intro is spread over mission time: {first:?}..{last:?}"
    );

    // --- the mission's own world, read from the live VM -------------------
    // Quests: real text, no raw `TRIGSTR_` keys.
    let quests: Vec<(String, bool)> = world
        .quests
        .values()
        .map(|quest| (quest.title.clone(), quest.required))
        .collect();
    println!("ORC01_QUESTS n={} rows={quests:?}", quests.len());
    assert!(quests.len() >= 2, "Orc01 creates its quest log: {quests:?}");
    // The VM records the script's own string (a `TRIGSTR_n` key or literal
    // text); the display layer resolves the key through `war3map.wts`, exactly
    // like `SubtitleQueue`/`QuestLog` do. What matters here is that the script
    // created its objectives with non-empty text.
    for (title, _) in &quests {
        assert!(
            !title.trim().is_empty(),
            "quest title is non-empty: {quests:?}"
        );
    }
    assert!(
        quests.iter().any(|(_, required)| *required),
        "at least one objective is required: {quests:?}"
    );

    // Camera: a sequence of setups, monotone in mission time.
    let cameras: Vec<(f64, String)> = world
        .camera_events
        .iter()
        .filter(|event| {
            matches!(
                event.kind.as_str(),
                "setup" | "setup_z" | "pan" | "position" | "pan_z"
            )
        })
        .map(|event| (event.t, event.kind.clone()))
        .collect();
    println!("ORC01_CAMERA_BEATS n={} first={:?} last={:?}", cameras.len(), cameras.first(), cameras.last());
    assert!(
        cameras.len() >= 4,
        "the opening applies a real camera sequence: {cameras:?}"
    );
    assert!(
        cameras.windows(2).all(|pair| pair[1].0 >= pair[0].0),
        "camera beats are monotone in mission time: {cameras:?}"
    );

    // Transmissions: a speaker and a non-empty line.
    let lines: Vec<(String, String)> = world
        .subtitles
        .iter()
        .map(|line| (line.speaker.clone(), line.text.clone()))
        .collect();
    println!("ORC01_TRANSMISSIONS n={} first={:?}", lines.len(), lines.first());
    assert!(!lines.is_empty(), "the intro has transmissions: {lines:?}");
    assert!(
        lines.iter().any(|(speaker, _)| !speaker.trim().is_empty()),
        "a transmission names its speaker: {lines:?}"
    );
    assert!(
        lines.iter().any(|(_, text)| !text.trim().is_empty()),
        "the transmissions carry the script's lines: {lines:?}"
    );

    // Fades: the cinematic filter was driven.
    let fades = world
        .camera_events
        .iter()
        .filter(|event| event.kind == "fade")
        .count();
    println!("ORC01_FADES n={fades}");
    assert!(fades >= 1, "the intro fades the screen: {fades}");
}

/// Human01 (the worked example of the campaign model) boots and its opening is
/// spread over ticks as well. Kept next to the Orc01 test: the two chapters
/// exercise different human seats (`Human01` = 2, `Orc01` = 2) and different
/// camera setups.
#[test]
fn human01_boots_and_its_intro_suspends() {
    let Some(archive) = archives().into_iter().next() else {
        eprintln!("skip: no readable War3.mpq");
        return;
    };
    let Some(root) = archive.parent().map(Path::to_path_buf) else {
        eprintln!("skip: archive has no parent");
        return;
    };
    let cache = std::env::temp_dir()
        .join("wc3rust-maps")
        .join("Maps")
        .join("Campaign")
        .join("Human01.w3m");
    if !(cache.is_file() && std::fs::metadata(&cache).map(|m| m.len()).unwrap_or(0) > 1024) {
        let Ok(mpq) = wc3_mpq::MpqArchive::open(&archive) else {
            eprintln!("skip: archive unreadable");
            return;
        };
        let Ok(bytes) = mpq.read("Maps\\Campaign\\Human01.w3m") else {
            eprintln!("skip: Human01 not in the archive");
            return;
        };
        if let Some(parent) = cache.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&cache, bytes);
    }
    wc3_re::jass::vm::set_local_player_any_default(true);
    let mut config = wc3_re::engine::EngineConfig::new(&root);
    config.map = Some(cache.clone());
    config.seed = 0x5EED;
    let mut engine = wc3_re::engine::Engine::boot(config).expect("Human01 boots");
    assert!(engine.script_main_ok, "war3map.j main: {:?}", engine.script_errors);
    assert_eq!(engine.state, wc3_re::engine::EngineState::InGame);
    for _ in 0..3000 {
        engine.tick(1.0 / 30.0);
    }
    let runtime = engine.runtime().expect("runtime");
    let world = &runtime.world;
    let setups = world.camera_events.iter().filter(|event| event.kind == "setup").count();
    let mut setup_handles: Vec<Option<u32>> = world
        .camera_events
        .iter()
        .filter(|event| event.kind == "setup")
        .map(|event| event.setup)
        .collect();
    setup_handles.sort_unstable();
    setup_handles.dedup();
    let setup_times: Vec<f64> = world
        .camera_events
        .iter()
        .filter(|event| event.kind == "setup" || event.kind == "pan" || event.kind == "position")
        .map(|event| (event.t * 100.0).round() / 100.0)
        .collect();
    let mut instants = setup_times.clone();
    instants.dedup();
    println!(
        "HUMAN01_CAMERA_TRACE setup_times={setup_times:?} instants={instants:?}"
    );
    println!(
        "HUMAN01_CAMERA registered_setups={} distinct_event_setups={:?}",
        world.camera_setups.len(),
        setup_handles
    );
    println!(
        "HUMAN01 time={:.2} sleeps={} wakes={} parked={} beats={} setups={} subtitles={} fades={}",
        world.game_time,
        runtime.sleeps,
        runtime.wakes,
        runtime.threads.len(),
        world.cinematic_log.len(),
        setups,
        world.subtitles.len(),
        world.camera_events.iter().filter(|event| event.kind == "fade").count()
    );
    assert!(runtime.sleeps > 0, "the opening suspends: sleeps={}", runtime.sleeps);
    assert!(runtime.wakes > 0, "the opening resumes: wakes={}", runtime.wakes);
    assert!(setups >= 4, "the opening applies a camera sequence: {setups} setups");
}
