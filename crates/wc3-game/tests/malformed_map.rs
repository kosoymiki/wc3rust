//! Malformed map bundles must load as an empty scene, never as a panic.
//!
//! The parsers are covered byte-for-byte in
//! `wc3-formats/tests/malformed_inputs.rs`. This file covers the layer above
//! them: `spawn_map_scene` / `apply_map_pathing` / `mask_water` /
//! `map_player_slots` receiving the *result* of a truncated map — missing
//! sections, a pathing grid shorter than `width * height`, a `.w3i` player
//! count that lies.
//!
//! These cases are cheap on purpose: a `Sim` plus hand-built `MapBundle`s. No
//! `Assets::load`, so peak RSS stays in the low tens of MB and the file can run
//! next to other agents on the shared device.

use glam::Vec2;
use wc3_formats::w3e::Terrain;
use wc3_formats::w3i::MapInfo;
use wc3_formats::wpm::PathingMap;
use wc3_game::sim_bridge::{
    apply_map_pathing, map_player_slots, mask_water, spawn_map_scene,
};
use wc3_re::mapfiles::MapBundle;
use wc3_sim::Sim;

/// Bundle with every parsed section absent: the shape a truncated archive
/// produces after the parsers rejected the bytes.
fn empty_bundle() -> MapBundle {
    MapBundle {
        path: std::path::PathBuf::from("<malformed>"),
        mpq_base: 0,
        entries: Vec::new(),
        listfile: Vec::new(),
        script: None,
        terrain: None,
        info: None,
        pathing: None,
        wts: None,
        objects: Default::default(),
        doodads: None,
        units: None,
        errors: Vec::new(),
    }
}

fn sim() -> Sim {
    Sim::new(0xBAD_5EED, 64, 64)
}

#[test]
fn spawn_map_scene_with_no_sections_is_an_empty_scene() {
    let mut sim = sim();
    let spawns = spawn_map_scene(&mut sim, &empty_bundle());
    assert_eq!(spawns.units, 0);
    assert_eq!(spawns.destructables, 0);
    assert_eq!(spawns.start_locations, 0);
    assert_eq!(spawns.gold_mines, 0);
    assert!(spawns.start_points.is_empty());
    assert!(spawns.melee.is_none());
    assert!(spawns.unit_census.is_empty());
    assert!(spawns.doodad_census.is_empty());
}

#[test]
fn truncated_payloads_parse_to_none_and_still_spawn_nothing() {
    // The bytes a half-downloaded map yields: a valid signature and nothing
    // else. The parsers reject them; the scene must survive the `None`s.
    let mut bundle = empty_bundle();
    bundle.terrain = Terrain::parse(b"W3E!").ok();
    bundle.info = MapInfo::parse(b"war3").ok();
    bundle.pathing = PathingMap::parse(b"MP3W").ok();
    bundle.doodads = wc3_formats::map::parse_doodads(truncated_doo()).ok();
    bundle.units = wc3_formats::map::parse_units(truncated_doo()).ok();
    assert!(bundle.terrain.is_none());
    assert!(bundle.info.is_none());
    assert!(bundle.pathing.is_none());
    assert!(bundle.doodads.is_none() || bundle.doodads.as_ref().is_some_and(Vec::is_empty));
    assert!(bundle.units.is_none() || bundle.units.as_ref().is_some_and(Vec::is_empty));

    let mut sim = sim();
    let spawns = spawn_map_scene(&mut sim, &bundle);
    assert_eq!(spawns.units, 0);
    assert_eq!(spawns.start_locations, 0);
}

/// A `.doo` header that declares one record but stops inside it.
fn truncated_doo() -> &'static [u8] {
    &[
        b'W', b'3', b'd', b'o', // magic
        8, 0, 0, 0, // version
        0, 0, 0, 0, // subversion
        1, 0, 0, 0, // count = 1
        b'A', b'T', b't', b'r', // id, then EOF
    ]
}

#[test]
fn pathing_grid_shorter_than_its_dimensions_never_panics() {
    // A truncated WPM that still parsed: width/height say 32x32, the cell array
    // holds 4 bytes. `walkable(x, y)` is total, so the game grid is built from
    // defaults instead of indexing the short array.
    let pathing = PathingMap {
        width: 32,
        height: 32,
        cells: vec![0u8; 4],
        trailing_bytes: 0,
    };
    let mut bundle = empty_bundle();
    bundle.pathing = Some(pathing);

    let mut sim = sim();
    let spawns = spawn_map_scene(&mut sim, &bundle);
    assert_eq!(spawns.units, 0);

    // The same bundle through the direct entry point.
    apply_map_pathing(&mut sim, &bundle);

    // `mask_water` is public and caller-supplied slices reach it; a grid
    // shorter than width*height must skip, not index.
    let before = wc3_formats::diag::count("pathing_grid_short");
    let mut short = vec![true; 10];
    let masked = mask_water(32, 32, 32.0, &mut short, |_, _| true);
    assert_eq!(masked, 10, "only the cells that exist are touched");
    let after = wc3_formats::diag::count("pathing_grid_short");
    assert!(
        after > before,
        "the short grid must be reported (before={before} after={after})"
    );
}

#[test]
fn w3i_with_a_lying_player_count_yields_no_slots() {
    // v23 tail: the fixed-size region the reader walks, then the player count.
    // A count of 1e9 is rejected by the 0..=24 guard, so no player block is
    // read and the caller keeps its fallback.
    let mut tail = vec![0u8; 8 * 4 + 4 * 4 + 8 + 5 + 4 + 1 + 4 + 1 + 4 + 20 + 4 + 1 + 5];
    tail.extend_from_slice(&1_000_000_000i32.to_le_bytes());
    let info = MapInfo {
        version: 23,
        raw_tail: tail,
        ..MapInfo::default()
    };
    let mut bundle = empty_bundle();
    bundle.info = Some(info);

    let slots = map_player_slots(&bundle);
    assert!(slots.is_empty(), "a lying player count must yield no slots");

    // The scene still loads (the melee shortcut simply stays off).
    let mut sim = sim();
    let spawns = spawn_map_scene(&mut sim, &bundle);
    assert!(spawns.melee.is_none());
}

#[test]
fn doodad_list_with_unknown_and_short_ids_is_censused_not_dropped() {
    // A `.doo` that parsed but names ids the data set does not know: each is
    // counted per id, the scene keeps going.
    let mut bundle = empty_bundle();
    bundle.doodads = Some(vec![
        wc3_formats::map::PlacedDoodad {
            type_id: *b"????",
            variation: 0,
            position: Vec2::new(100.0, 100.0).extend(0.0),
            rotation: 0.0,
            scale: Vec2::ONE.extend(1.0),
            state: 0,
            life: 100,
        },
        wc3_formats::map::PlacedDoodad {
            type_id: [0x41, 0x54, 0x74, 0x72],
            variation: 0,
            position: Vec2::new(200.0, 200.0).extend(0.0),
            rotation: 0.0,
            scale: Vec2::ONE.extend(1.0),
            state: 0,
            life: 100,
        },
    ]);
    let mut sim = sim();
    let spawns = spawn_map_scene(&mut sim, &bundle);
    assert_eq!(spawns.doodad_census.len(), 2, "both ids are censused");
    assert!(
        spawns
            .doodad_census
            .get("????")
            .is_some_and(|census| census.unknown_def == 1),
        "the unknown id is reported as no-data, not silently dropped"
    );
}
