//! Lane 509 acceptance (ASSETS + MAPS): MPQ layering, campaign + melee map
//! headers, HUD pack/fonts and the packaged asset hash.
//!
//! The device install is the target: `War3.mpq` + `War3x.mpq` +
//! `War3xlocal.mpq` next to `Maps/` and `hud_pack/`. Every test skips with an
//! explicit `skip:` line when the install is absent, so a desktop checkout can
//! still run the suite; on the device the assertions are live.
//!
//! Artifact lines (grep targets):
//! * `MPQ_LAYERS …`                      one line per mounted archive layer;
//! * `MAP_HEADER …`                      one line per registered map;
//! * `ORC01_ASSETS …`                    the Orc01 acceptance line;
//! * `HUD_PACK_CHECK …`                  pack assets + hash + fonts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wc3_game::assets::Assets;
use wc3_game::campaign;
use wc3_game::map_registry;

/// Candidate install roots, most specific first.
fn install_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("H2_WC3_INSTALL") {
        roots.push(PathBuf::from(dir));
    }
    roots.push(PathBuf::from("/data/data/org.herofolends.p0/files"));
    roots.push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    roots.push(PathBuf::from(
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso",
    ));
    roots
}

/// First root that carries a readable `War3.mpq`.
///
/// The operator's install lives under the app's private data dir (`0700` for
/// other uids): a plain user run falls through to the readable ISO copy while a
/// `su -c` run (root authority) targets the real install.
fn install_root() -> Option<PathBuf> {
    let root = install_roots().into_iter().find(|root| {
        let war3 = root.join("War3.mpq");
        war3.is_file() && std::fs::File::open(&war3).is_ok()
    })?;
    println!(
        "INSTALL root={} uid={}",
        root.display(),
        unsafe { libc::getuid() }
    );
    Some(root)
}

fn hud_pack_root() -> Option<PathBuf> {
    wc3_game::hudskin::pack::find_installed()
}

/// Debug lines that mean the loader hit a file it could not read. The Orc01
/// acceptance is "zero missing files in the layer log", so the list is explicit
/// and grows only with a named reason.
fn missing_line(line: &str) -> bool {
    const NEEDLES: [&str; 11] = [
        "map_open_failed=",
        "tile_failed=",
        "texture_failed=",
        "model_failed=",
        "objects_failed=",
        "objects_storm_failed=",
        "storm_open_failed=",
        "storm_assetdb_failed=",
        "mpq_open_failed=",
        "mpq_layered_failed=",
        "ui_atlas=missing",
    ];
    NEEDLES.iter().any(|needle| line.contains(needle))
}

/// Split an `Assets::debug` `storm_layers=[…]` line into labels.
fn storm_layers(assets: &Assets) -> Vec<String> {
    let line = assets
        .debug
        .iter()
        .find(|line| line.starts_with("storm_layers="))
        .unwrap_or_else(|| panic!("no storm_layers= line in {:?}", assets.debug));
    line.trim_start_matches("storm_layers=")
        .trim_matches(['[', ']'])
        .split(", ")
        .map(|label| label.trim_matches('"').to_string())
        .collect()
}

/// Every mounted archive opens and reports its own listfile count, in install
/// priority order. Evidence for the layer log.
#[test]
fn mpq_layers_include_base_expansion_and_locale() {
    let Some(root) = install_root() else {
        eprintln!("skip: no WC3 install (set H2_WC3_INSTALL)");
        return;
    };
    let mut mounted = Vec::new();
    for name in ["War3.mpq", "War3x.mpq", "War3xlocal.mpq"] {
        let path = root.join(name);
        if !path.is_file() {
            eprintln!("skip layer {name}: absent in {}", root.display());
            continue;
        }
        let archive = wc3_mpq::MpqArchive::open(&path).expect("archive opens");
        assert!(
            archive.listfile().len() > 1000,
            "{name}: listfile={} (a real install layer has thousands)",
            archive.listfile().len()
        );
        // A file that every layer answers proves the reader works per layer.
        let probe = "Units\\UnitData.slk";
        if archive.contains(probe) {
            let bytes = archive.read(probe).expect("layer reads UnitData.slk");
            assert!(bytes.len() > 1000, "{name}: UnitData.slk is truncated");
        }
        println!(
            "MPQ_LAYERS archive={name} listfile={} blocks={} locals={}",
            archive.listfile().len(),
            archive.block_count(),
            archive.unnamed_block_count()
        );
        mounted.push(name.to_string());
    }
    assert!(
        mounted.iter().any(|name| name == "War3.mpq"),
        "the base archive must mount"
    );
    // A missing file is an explicit `FileNotFound`, never empty bytes.
    let war3 = wc3_mpq::MpqArchive::open(root.join("War3.mpq")).expect("War3.mpq");
    let err = war3
        .read("Units\\NoSuchFile_509.mdx")
        .expect_err("a missing file must fail");
    assert!(
        matches!(err, wc3_mpq::MpqError::FileNotFound(_)),
        "missing file yielded {err:?}, not FileNotFound"
    );
    println!("MPQ_LAYERS missing_probe=FileNotFound layers={mounted:?}");
}

/// Orc01 comes out of `War3.mpq` (`Maps\Campaign\Orc01.w3m`), not from a
/// hand-copied file, and loads with zero missing files, the pack skin and the
/// archive fonts.
#[test]
fn orc01_loads_from_the_mpq_with_zero_missing_files() {
    let Some(root) = install_root() else {
        eprintln!("skip: no WC3 install (set H2_WC3_INSTALL)");
        return;
    };
    let war3 = root.join("War3.mpq");
    let entries = campaign::archive_campaign_entries(&war3);
    let entry = entries
        .iter()
        .find(|(_, id)| id.eq_ignore_ascii_case("Orc01"))
        .unwrap_or_else(|| panic!("Maps\\Campaign\\Orc01.w3m not in {} ({entries:?})", war3.display()));
    let (map_path, bytes) =
        campaign::extract_archive_entry(&war3, &entry.0).expect("extract Orc01 from the MPQ");
    assert!(bytes > 100_000, "Orc01 is {bytes} bytes");
    assert!(
        !map_path.starts_with(&root),
        "Orc01 must load from the archive cache ({}), not the install directory",
        map_path.display()
    );

    let assets = Assets::load(Some(&war3), Some(&map_path));
    let layers = storm_layers(&assets);
    assert!(
        layers.iter().any(|layer| layer == "War3.mpq"),
        "War3.mpq missing from {layers:?}"
    );
    assert!(
        layers.iter().any(|layer| layer == "War3x.mpq"),
        "War3x.mpq (TFT tables) missing from {layers:?}"
    );
    // Priority: the locale archive overrides the expansion, which overrides the
    // RoC base (first mount wins in `StormFs`).
    let index_of = |name: &str| layers.iter().position(|layer| layer == name);
    if let (Some(local), Some(tft), Some(roc)) = (
        index_of("War3xlocal.mpq"),
        index_of("War3x.mpq"),
        index_of("War3.mpq"),
    ) {
        assert!(
            local < tft && tft < roc,
            "layer priority must be locale > expansion > base: {layers:?}"
        );
    }
    assert!(
        assets.debug.iter().any(|line| line.starts_with("map=")),
        "map= line missing: {:?}",
        assets.debug
    );
    let terrain = assets.terrain.as_ref().expect("Orc01 terrain");
    assert!(terrain.width > 0 && terrain.height > 0);
    assert!(!assets.terrain_tiles.is_empty(), "no terrain tiles decoded");
    assert!(assets.world_size.x > 0.0 && assets.world_size.y > 0.0);
    let objects = assets.objects.as_ref().expect("Orc01 object database");
    assert!(
        objects.missing().is_empty(),
        "data tables missing: {:?}",
        objects.missing()
    );
    // `Localizer::missing_files` is profile *coverage*: retail does not ship
    // every `<Race><Strings|Func>.txt` combination (none of the three archives
    // carries `Units\UnitGlobalFunc.txt`), so it is reported, not asserted to
    // be empty. The Orc01 names must still resolve through the localisation
    // layer, which is what proves War3xlocal is mounted and decoded.
    let profiles_missing = objects.localizer().missing_files();
    let hall = assets
        .name_for_def("ogre")
        .unwrap_or_else(|| panic!("orc hall name unresolved (profiles_missing={profiles_missing:?})"));
    assert!(
        hall.chars()
            .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch)),
        "orc hall name {hall:?} is not the localised (Cyrillic) string; War3xlocal text was not read"
    );
    assert!(
        !assets.models.is_empty() && assets.model_by_def.len() > 10,
        "def models={} by_def={}",
        assets.models.len(),
        assets.model_by_def.len()
    );
    let missing: Vec<&String> = assets.debug.iter().filter(|line| missing_line(line)).collect();
    assert!(
        missing.is_empty(),
        "Orc01 load reported missing files: {missing:#?}"
    );
    let meshless = assets
        .debug
        .iter()
        .filter(|line| line.starts_with("model_meshless="))
        .count();
    let skin = assets
        .hud_skin
        .as_ref()
        .unwrap_or_else(|| panic!("HUD pack skin unresolved: {:?}", assets.debug));
    let pack = skin
        .pack
        .as_ref()
        .expect("the Orc01 skin must come from HudPack, not the atlas");
    assert!(
        pack.hash_files > 0,
        "the installed pack carries SHA256SUMS.txt but no file was hashed"
    );
    assert!(
        pack.font_textures.iter().all(|slot| *slot > 0),
        "pack font atlases unbound: {:?}",
        pack.font_textures
    );
    assert!(
        pack.root.join("SHA256SUMS.txt").is_file(),
        "HUD pack {} has no packaged hash manifest",
        pack.root.display()
    );
    if root.join("hud_pack").is_dir() {
        assert!(
            pack.root.starts_with(&root),
            "HUD pack {} is not the install's own pack under {}",
            pack.root.display(),
            root.display()
        );
    }
    let pack_line = assets
        .debug
        .iter()
        .find(|line| line.starts_with("HUD_PACK root="))
        .unwrap_or_else(|| panic!("no HUD_PACK line in {:?}", assets.debug));
    println!(
        "ORC01_ASSETS entry={} cache={} bytes={} layers={:?} terrain={}x{} tiles={} units_placed={} models={} scope={} profiles_missing={} hall={:?} missing=0",
        entry.0,
        map_path.display(),
        bytes,
        layers,
        terrain.width,
        terrain.height,
        assets.terrain_tiles.len(),
        assets.start_locations,
        assets.model_by_def.len(),
        assets.model_scope.summary(),
        profiles_missing.len(),
        hall
    );
    println!("ORC01_ASSETS model_meshless={meshless} debug_lines={}", assets.debug.len());
    println!("ORC01_ASSETS {pack_line}");
    println!(
        "ORC01_ASSETS hud_skin=pack hash={} fonts={:?} font_files={}",
        pack.hash_files,
        pack.font_textures,
        pack.font_files.len()
    );
}

/// Every registered map's header opens with a non-empty terrain. Placed units
/// are asserted when the map ships `war3mapUnits.doo`; script-only chapters
/// must instead create units from `war3map.j` (campaign maps do).
#[test]
fn every_registered_map_header_loads() {
    let Some(root) = install_root() else {
        eprintln!("skip: no WC3 install (set H2_WC3_INSTALL)");
        return;
    };
    let mut registry = map_registry::build_install(&root, "2026-11-18");
    // The device `Maps/` tree is byte-identical to `donors/maps/melee` (38
    // files, sha256 equal) but lives under the app's private data dir (0700 for
    // other uids). A non-root run cannot list it; the curated tree is then the
    // same 38 containers, so the melee half of the proof stays real.
    let mut melee_source = format!("{}/Maps", root.display());
    if map_registry::local_maps(&root.join("Maps")).is_empty() {
        melee_source = format!(
            "{} (device Maps unreadable to uid {})",
            map_registry::MAPS_DIR,
            unsafe { libc::getuid() }
        );
        for path in map_registry::local_maps(&map_registry::Registry::maps_dir()) {
            match map_registry::census(&path, &map_registry::Registry::repo_root()) {
                Ok(entry) => registry.maps.push(entry),
                Err(error) => panic!("{}: {error}", path.display()),
            }
        }
    }
    println!("MAP_HEADER melee_source={melee_source}");
    assert!(
        registry.maps.len() >= 38,
        "install registry has {} maps, expected the 38 melee maps plus campaign chapters",
        registry.maps.len()
    );
    let mut per_tileset: BTreeMap<String, usize> = BTreeMap::new();
    let mut campaign = 0usize;
    let mut melee = 0usize;
    for map in &registry.maps {
        let path = map_registry::resolved_path(map);
        let bundle = wc3_re::mapfiles::MapBundle::open(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", map.file));
        let terrain = bundle
            .terrain
            .as_ref()
            .unwrap_or_else(|| panic!("{}: no terrain", map.file));
        assert!(
            terrain.width > 0 && terrain.height > 0,
            "{}: terrain {}x{}",
            map.file,
            terrain.width,
            terrain.height
        );
        assert!(!terrain.tiles.is_empty(), "{}: empty terrain", map.file);
        let placed = map.units_placed;
        let scripted = map.script_create_unit_calls;
        assert!(
            placed > 0 || scripted > 0,
            "{}: no placed units and no CreateUnit calls",
            map.file
        );
        assert!(
            map.players >= 2 || map.script_create_unit_calls > 0,
            "{}: players={} with no script",
            map.file,
            map.players
        );
        if map.origin.starts_with("mpq:") {
            campaign += 1;
        } else if map.melee {
            melee += 1;
        }
        *per_tileset.entry(map.tileset.clone()).or_default() += 1;
        println!(
            "MAP_HEADER id={} origin={} format={} terrain={}x{} tileset={} players={} units={} script_calls={} sha256={}",
            map.id,
            map.origin,
            map.format,
            terrain.width,
            terrain.height,
            map.tileset,
            map.players,
            placed,
            scripted,
            &map.sha256[..12]
        );
    }
    assert!(campaign >= 30, "campaign chapters registered: {campaign}");
    assert!(melee >= 30, "melee maps registered: {melee}");
    if std::env::var("H2_MAPS_INSTALL_BUILD").ok().as_deref() == Some("1") {
        let path = map_registry::install_registry_path();
        map_registry::write_registry(&registry, &path).expect("write install registry");
        println!("MAP_HEADER wrote {}", path.display());
    }
    println!(
        "MAP_HEADER_DONE total={} melee={} campaign={} tilesets={:?}",
        registry.maps.len(),
        melee,
        campaign,
        per_tileset
    );
}

/// The map list must show the real player count: `players_for_file` equals the
/// census `sloc` count (floored at one) for every sampled map, and the sample
/// is not the constant 2 the menu used to draw.
#[test]
fn players_for_file_matches_the_census() {
    let Some(root) = install_root() else {
        eprintln!("skip: no WC3 install (set H2_WC3_INSTALL)");
        return;
    };
    let mut maps = map_registry::local_maps(&root.join("Maps"));
    let mut source = format!("{}/Maps", root.display());
    if maps.is_empty() {
        source = map_registry::MAPS_DIR.to_string();
        maps = map_registry::local_maps(&map_registry::Registry::maps_dir());
    }
    assert!(maps.len() >= 30, "install Maps/ has {} maps", maps.len());
    let mut distinct = std::collections::BTreeSet::new();
    for path in maps.iter().take(8) {
        let census = map_registry::census_with_origin(path, &root, "install:test")
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let players = map_registry::players_for_file(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(
            players as usize,
            census.start_locations.clamp(1, u8::MAX as usize),
            "{}",
            path.display()
        );
        distinct.insert(players);
        println!(
            "PLAYERS source={source} file={} players={} sloc={}",
            path.file_name().unwrap().to_string_lossy(),
            players,
            census.start_locations
        );
    }
    assert!(
        distinct.len() > 1,
        "the sample shows one player count: {distinct:?}"
    );
}

/// The HUD pack listed in `manifest.json` is fully present, matches
/// `SHA256SUMS.txt`, and its fonts rasterise.
#[test]
fn hud_pack_assets_match_the_packaged_hash() {
    let Some(root) = hud_pack_root() else {
        eprintln!("skip: no hud_pack directory (set WC3RUST_HUD_PACK)");
        return;
    };
    let report = wc3_game::hudskin::pack::verify_pack(&root).expect("pack verifies");
    assert!(report.assets >= 90, "manifest assets={}", report.assets);
    assert!(report.files >= 90, "hashed files={}", report.files);
    assert_eq!(report.mismatched, 0, "hash mismatches: {:?}", report.mismatches);
    assert!(report.fonts >= 5, "font files={}", report.fonts);
    let fonts = wc3_game::hud::pack_fonts(&root).expect("pack fonts rasterise");
    for (slot, path) in fonts.paths.iter().enumerate() {
        assert!(!path.is_empty(), "font slot {slot} unresolved");
    }
    println!(
        "HUD_PACK_CHECK root={} assets={} files={} fonts={} hashed_bytes={} primary={}",
        root.display(),
        report.assets,
        report.files,
        report.fonts,
        report.hashed_bytes,
        fonts.primary_source
    );
}
