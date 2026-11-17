//! Product gate: every TerrainArt tile resolves from the operator's install.
//!
//! `PARITY_MATRIX.md` R4 and the asset catalog both state 15 tilesets / 111
//! TerrainArt BLPs. The renderer only ever samples the map's ground tags, so a
//! bad tileset path could silently degrade to `source=fallback`; this test parses
//! `TerrainArt\Terrain.slk` from the real archive and opens **every** tile BLP.
//!
//! Skips (with a line) when the local install is not mounted, so CI without the
//! game files still passes.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn all_terrainart_tiles_resolve() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/agent-assets/wc3-install");
    if !root.join("War3.mpq").exists() {
        println!("skip: no WC3 install at {}", root.display());
        return;
    }
    let storm = wc3_re::storm::StormFs::open_wc3_install(&root).expect("open WC3 install");
    let bytes = storm
        .read("TerrainArt\\Terrain.slk")
        .expect("TerrainArt\\Terrain.slk");
    let mut art = wc3_re::terrain::TerrainArt::default();
    art.load_terrain(&bytes).expect("parse Terrain.slk");
    let paths = art.texture_paths();
    assert!(!paths.is_empty(), "Terrain.slk has no tiles");

    let mut tilesets = BTreeSet::new();
    let mut unique_files = BTreeSet::new();
    let mut missing = Vec::new();
    for (id, path) in &paths {
        let normalized = path.to_ascii_lowercase();
        if let Some((dir, _)) = normalized.rsplit_once('\\') {
            tilesets.insert(dir.to_string());
        }
        unique_files.insert(normalized);
        if let Err(error) = storm.read(path) {
            missing.push(format!("{id}:{path} ({error})"));
        }
    }
    println!(
        "TERRAINART rows={} unique_files={} tilesets={} missing={}",
        paths.len(),
        unique_files.len(),
        tilesets.len(),
        missing.len()
    );
    assert!(
        missing.is_empty(),
        "{} TerrainArt tiles unresolved: {missing:?}",
        missing.len()
    );
    // The catalog baseline is the RoC archive (111 unique BLPs / 15 tilesets).
    // The combined install also mounts TFT (War3x/War3xlocal), which adds rows
    // and tilesets; every one of those must resolve too (checked above).
    let mut roc = wc3_re::storm::StormFs::new();
    roc.add_archive("War3", root.join("War3.mpq"))
        .expect("open War3.mpq");
    let roc_bytes = roc
        .read("TerrainArt\\Terrain.slk")
        .expect("RoC Terrain.slk");
    let mut roc_art = wc3_re::terrain::TerrainArt::default();
    roc_art.load_terrain(&roc_bytes).expect("parse RoC Terrain.slk");
    let roc_paths = roc_art.texture_paths();
    let roc_files: BTreeSet<String> = roc_paths
        .iter()
        .map(|(_, path)| path.to_ascii_lowercase())
        .collect();
    let roc_dirs: BTreeSet<String> = roc_files
        .iter()
        .filter_map(|path| path.rsplit_once('\\').map(|(dir, _)| dir.to_string()))
        .collect();
    println!(
        "TERRAINART_ROC rows={} unique_files={} tilesets={}",
        roc_paths.len(),
        roc_files.len(),
        roc_dirs.len()
    );
    // PARITY_MATRIX R4 records "Terrain.slk 98 tiles"; this is the measured RoC
    // baseline. The asset catalog's 111/15 counts every TerrainArt BLP in the
    // layered install, including files no Terrain.slk row references.
    assert_eq!(roc_files.len(), 98, "RoC unique TerrainArt BLPs (matrix: 98 tiles)");
    assert_eq!(roc_dirs.len(), 13, "RoC tileset dirs: {roc_dirs:?}");
}
