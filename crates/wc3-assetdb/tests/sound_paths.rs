//! Retail sound-table paths that ship under a different directory.
//!
//! The six mappings were found by listfile analysis (unique basename, stale
//! table directory) and must resolve to the canonical archive spelling and
//! decode. Nothing is guessed: a path with no unique candidate stays absent.

use std::path::PathBuf;

use wc3_assetdb::{AssetDb, Resolution};

const STALE: &[(&str, &str)] = &[
    (
        "Buildings\\Orc\\PigFarm\\PigFarmWhat1.wav",
        "buildings\\Other\\PigFarm\\PigFarmWhat1.wav",
    ),
    (
        "Buildings\\NightElf\\AncientOfTheWild\\AncientOfTheWildAttack1.wav",
        "buildings\\NightElf\\AncientOfWind\\AncientOfTheWildAttack1.wav",
    ),
    (
        "Buildings\\NightElf\\AncientOfTheWild\\AncientOfTheWildAttack2.wav",
        "buildings\\NightElf\\AncientOfWind\\AncientOfTheWildAttack2.wav",
    ),
    (
        "Buildings\\NightElf\\AncientOfTheEarth\\AncientOfTheEarthAttack1.wav",
        "buildings\\NightElf\\AncientOfLore\\AncientOfTheEarthAttack1.wav",
    ),
    (
        "Buildings\\NightElf\\AncientOfTheMoon\\AncientOfTheMoonAttack1.wav",
        "buildings\\NightElf\\AncientOfWar\\AncientOfTheMoonAttack1.wav",
    ),
    (
        "Units\\NightElf\\AncientProtector\\AncientProtectorAttack2.wav",
        "buildings\\NightElf\\AncientProtector\\AncientProtectorAttack2.wav",
    ),
];

fn open_db() -> Option<AssetDb> {
    let mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
        });
    if !mpq.is_file() {
        return None;
    }
    let index = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/agent-assets/war3.index");
    match index.is_file() {
        true => AssetDb::open_mpq_best(&mpq, &index).ok().map(|(db, _)| db),
        false => AssetDb::open_mpq(&mpq).ok(),
    }
}

#[test]
fn six_stale_table_paths_resolve_and_decode() {
    let Some(mut db) = open_db() else {
        eprintln!("skipping: War3.mpq not available");
        return;
    };
    let mut resolved = 0usize;
    for (requested, expected) in STALE {
        match db.resolve_asset(requested) {
            Resolution::ListfileMatch { path, .. } => {
                assert!(
                    path.eq_ignore_ascii_case(expected),
                    "{requested}: got {path}, expected {expected}"
                );
                let sound = db
                    .sound_shared(&path)
                    .unwrap_or_else(|err| panic!("{requested}: decode {path}: {err}"));
                assert!(sound.frames() > 100, "{requested}: frames {}", sound.frames());
                resolved += 1;
            }
            other => panic!("{requested}: expected ListfileMatch, got {other:?}"),
        }
    }
    assert_eq!(resolved, STALE.len());
    println!("resolved {resolved} stale sound-table paths");
}

#[test]
fn absent_definition_is_not_fabricated() {
    let Some(mut db) = open_db() else {
        return;
    };
    let (sounds, _, _) = wc3_assetdb::audio_provider::load_sound_tables(&mut db);
    let def = sounds
        .get("HeroArchDruidWhat")
        .expect("retail table defines HeroArchDruidWhat");
    let mut any_resolved = false;
    for path in def.paths() {
        if let Some(canonical) = db.sound_path(&path) {
            any_resolved = true;
            assert!(db.sound_shared(&canonical).is_ok());
        }
    }
    assert!(
        !any_resolved,
        "HeroArchDruid is TFT content; nothing may resolve from War3.mpq"
    );
    // The gameplay hook must report the miss rather than inventing a file.
    assert!(db.sound_path("Units\\NightElf\\HeroArchDruid\\HeroArchDruidWhat1.wav").is_none());
}
