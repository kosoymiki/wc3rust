//! Diagnostic: what the typed object database actually resolves for the
//! starting units of the installed map (names, icons, menus, missing files).
use std::path::Path;
use wc3_game::assets::Assets;

#[test]
fn probe_starting_unit_card_data() {
    let mpq = Path::new("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq");
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if !mpq.is_file() || !map.is_file() {
        eprintln!("skip: install or map missing");
        return;
    }
    let assets = Assets::load(Some(mpq), Some(&map));
    let Some(objects) = assets.objects.as_ref() else {
        panic!("object database did not load");
    };
    eprintln!("missing files = {:?}", objects.missing());
    for id in ["opeo", "ogre", "otrb", "ofor", "obar", "oalt", "owtw", "osld", "obea", "hpea", "htow"] {
        eprintln!(
            "{id}: name={:?} icon={:?} model={:?} kind={:?}",
            assets.name_for_def(id),
            assets.icon_name_for_def(id),
            objects.object(id).and_then(|info| info.model.clone()),
            objects.object(id).map(|info| info.kind),
        );
    }
    // Portraits/card icons must be the unit's own, not the four starter kinds.
    for id in ["opeo", "hpea", "ogre", "htow", "oalt", "nftr"] {
        eprintln!(
            "icon {id}: type_icon={:?} name={:?} race={:?}",
            assets.ui.type_icon(id),
            assets.icon_name_for_def(id),
            assets.info_for_def(id).and_then(|info| info.race),
        );
    }
    assert_ne!(
        assets.ui.type_icon("opeo"),
        assets.ui.type_icon("hpea"),
        "an Orc peon must not reuse the Human peasant portrait"
    );
    assert!(assets.ui.type_icon("opeo").is_some(), "peon portrait loaded");
    assert!(assets.ui.type_icon("ogre").is_some(), "great hall icon loaded");
    for (producer, label) in [("ogre", "train"), ("opeo", "build")] {
        let menu = if label == "train" {
            assets.train_menu(producer)
        } else {
            assets.build_menu(producer)
        };
        eprintln!("{label} menu of {producer}: {} entries", menu.len());
        for entry in &menu {
            eprintln!(
                "  {} name={:?} icon={:?} gold={:?} lumber={:?} hotkey={:?}",
                entry.id, entry.name, entry.icon, entry.gold, entry.lumber, entry.hotkey
            );
        }
    }
}

#[test]
fn probe_start_positions() {
    let mpq = Path::new("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq");
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    if !mpq.is_file() || !map.is_file() {
        return;
    }
    let assets = Assets::load(Some(mpq), Some(&map));
    let game = wc3_game::game::Game::new(assets, 0x5EED);
    let mut workers = Vec::new();
    for unit in game.sim.alive_units().filter(|u| u.owner == game.player) {
        eprintln!(
            "{} def={} worker={} pos=({:.0},{:.0}) building={}",
            unit.id.0,
            unit.def().id,
            unit.def().stats.is_worker,
            unit.pos.x,
            unit.pos.y,
            unit.is_building()
        );
        if unit.def().stats.is_worker {
            workers.push(unit.pos);
        }
    }
    for i in 0..workers.len() {
        for j in (i + 1)..workers.len() {
            let d = (workers[i] - workers[j]).length();
            eprintln!("dist {i}-{j} = {d:.1}");
        }
    }
}
