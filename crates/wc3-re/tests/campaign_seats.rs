//! The campaign human seat, measured on the shipped chapters.
//!
//! `war3map.w3i` v18 (Reign of Chaos) carries no player records — Human01's
//! parsed `players` array is empty — so the campaign layer infers the seat. The
//! inference is load-bearing: Blizzard.j gates every camera and transmission on
//! `GetLocalPlayer() == whichPlayer`, and a wrong seat silently drops the whole
//! opening. This test pins the seats that were measured, so a "better" guess
//! that breaks them fails here instead of on the device.
//!
//! Both chapters run in one test on purpose: the JASS seat default is
//! process-wide, and two parallel tests would race on it.

use std::path::{Path, PathBuf};

fn archive() -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/agent-assets/wc3-install/War3.mpq");
    path.is_file().then_some(path)
}

fn chapter(root: &Path, id: &str) -> Option<PathBuf> {
    let map = std::env::temp_dir()
        .join("wc3rust-maps")
        .join("Maps")
        .join("Campaign")
        .join(format!("{id}.w3m"));
    if map.is_file() && std::fs::metadata(&map).map(|m| m.len()).unwrap_or(0) > 1024 {
        return Some(map);
    }
    let mpq = wc3_mpq::MpqArchive::open(root.join("War3.mpq")).ok()?;
    let bytes = mpq.read(&format!("Maps\\Campaign\\{id}.w3m")).ok()?;
    std::fs::create_dir_all(map.parent()?).ok()?;
    std::fs::write(&map, bytes).ok()?;
    Some(map)
}

/// Runs the chapter's script for `seat` (`None` = single-player wildcard) and
/// returns the `(camera setups, subtitles)` the opening produced.
fn opening_for(root: &Path, map: &Path, seat: Option<u32>) -> (usize, usize) {
    match seat {
        Some(seat) => {
            wc3_re::jass::vm::Runtime::set_default_local_player(seat);
            wc3_re::jass::vm::set_local_player_any_default(false);
        }
        None => wc3_re::jass::vm::set_local_player_any_default(true),
    }
    let mut config = wc3_re::engine::EngineConfig::new(root);
    config.map = Some(map.to_path_buf());
    config.seed = 0x5EED;
    let mut engine = wc3_re::engine::Engine::boot(config).expect("chapter boots");
    for _ in 0..1200 {
        engine.tick(1.0 / 30.0);
    }
    let world = &engine.runtime().expect("runtime").world;
    (
        world
            .camera_events
            .iter()
            .filter(|event| event.kind == "setup")
            .count(),
        world.subtitles.len(),
    )
}

#[test]
fn campaign_seats_are_measured() {
    let Some(archive) = archive() else {
        eprintln!("skip: no readable War3.mpq");
        return;
    };
    let root = archive.parent().unwrap().to_path_buf();

    // --- Human01: the opening belongs to seat 1 --------------------------
    let Some(human01) = chapter(&root, "Human01") else {
        eprintln!("skip: Human01 not in the archive");
        return;
    };
    let (setups, subtitles) = opening_for(&root, &human01, Some(1));
    println!("HUMAN01_SEAT seat=1 setups={setups} subtitles={subtitles}");
    assert!(setups >= 4, "seat 1 runs the camera sequence: {setups} setups");
    assert!(subtitles >= 1, "seat 1 hears the transmissions: {subtitles}");
    for wrong in [0u32, 2, 3] {
        let (setups, subtitles) = opening_for(&root, &human01, Some(wrong));
        println!("HUMAN01_SEAT seat={wrong} setups={setups} subtitles={subtitles}");
        assert_eq!(
            (setups, subtitles),
            (0, 0),
            "seat {wrong} is not the mission's human: {setups} setups, {subtitles} subtitles"
        );
    }
    let (setups, subtitles) = opening_for(&root, &human01, None);
    println!("HUMAN01_SEAT wildcard setups={setups} subtitles={subtitles}");
    assert!(
        setups >= 4 && subtitles >= 1,
        "the single-player wildcard runs the opening: {setups} setups, {subtitles} subtitles"
    );

    // --- Orc01: the wildcard runs it, and its marker names the seat ------
    let Some(orc01) = chapter(&root, "Orc01") else {
        eprintln!("skip: Orc01 not in the archive");
        return;
    };
    let (setups, subtitles) = opening_for(&root, &orc01, None);
    println!("ORC01_SEAT wildcard setups={setups} subtitles={subtitles}");
    assert!(setups >= 4, "the wildcard runs Orc01's camera: {setups}");
    assert!(subtitles >= 1, "the wildcard hears Orc01: {subtitles}");

    wc3_re::jass::vm::set_local_player_any_default(true);
    let mut config = wc3_re::engine::EngineConfig::new(&root);
    config.map = Some(orc01.clone());
    config.seed = 0x5EED;
    let engine = wc3_re::engine::Engine::boot(config).expect("boot");
    let world = &engine.runtime().expect("runtime").world;
    let opening = world.camera_setups.values().find_map(|setup| setup.dest);
    let mut markers: Vec<(f64, i32)> = world
        .units
        .values()
        .filter(|unit| unit.id.eq_ignore_ascii_case("sloc") && (0..12).contains(&unit.owner))
        .map(|unit| {
            let distance =
                opening.map_or(0.0, |(x, y)| (unit.x - x).powi(2) + (unit.y - y).powi(2));
            (distance, unit.owner)
        })
        .collect();
    markers.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
    let seat = markers.first().map(|(_, owner)| *owner);
    println!("ORC01_SEAT marker seat={seat:?} markers={markers:?}");
    assert!(
        matches!(seat, Some(0)),
        "Orc01's start marker nearest the opening shot is player 0 (Thrall)"
    );
}
