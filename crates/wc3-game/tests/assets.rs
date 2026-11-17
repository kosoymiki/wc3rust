//! Asset invariants on the real user archive. These lock the three format
//! roots that produced the "texture mess": BLP1 JPEG CMYK, MDX Z-up bounds and
//! GEOA-hidden geosets.
use std::path::{Path, PathBuf};

use wc3_game::Assets;
use wc3_sim::UnitKind;

fn data() -> Option<(PathBuf, PathBuf)> {
    let candidates = [
        PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq"),
        PathBuf::from("/data/data/org.herofolends.p0/files/War3.mpq"),
    ];
    let mpq = candidates.into_iter().find(|p| p.exists())?;
    let map = Path::new(
        "/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_map/Maps/Campaign/Human04.w3m",
    );
    let map = if map.exists() {
        map.to_path_buf()
    } else {
        PathBuf::from("/data/data/org.herofolends.p0/files/Human04.w3m")
    };
    if !map.exists() { None } else { Some((mpq, map)) }
}

#[test]
fn real_archive_has_correct_formats() {
    let Some((mpq, map)) = data() else {
        eprintln!("SKIP assets: user War3.mpq/map not present");
        return;
    };
    let assets = Assets::load(Some(&mpq), Some(&map));
    assert_eq!(assets.source, "user-mpq");
    assert!(assets.terrain_atlas.is_some(), "terrain atlas decoded");

    let footman = assets.model(UnitKind::Footman);
    assert!(!footman.fallback, "real Footman.mdx");
    assert!(footman.height > 80.0, "MDX is Z-up: height {}", footman.height);
    let mdx = footman.model.as_ref().unwrap();
    assert!(!mdx.geoset_animations.is_empty(), "GEOA parsed");

    let townhall = assets.model(UnitKind::TownHall);
    let townhall_index = assets
        .models
        .iter()
        .position(|m| m.kind == UnitKind::TownHall)
        .unwrap();
    let alphas = assets.mesh_alpha(townhall_index, None, 0);
    assert!(
        alphas.iter().any(|alpha| *alpha < 0.1),
        "GEOA hides at least one helper geoset: {alphas:?}"
    );
    // TownHall `Stand` has exactly one KGAO key at its start (0.0). Warcraft 3
    // treats a single in-range key as a constant, not as an animation, so the
    // building must stay visible instead of sampling the export-boundary key.
    let stand = townhall
        .sequences
        .iter()
        .find(|sequence| sequence.name.eq_ignore_ascii_case("Stand"))
        .expect("TownHall Stand sequence");
    let stand_alphas = assets.mesh_alpha(townhall_index, Some(stand), stand.start);
    assert!(
        stand_alphas.iter().filter(|alpha| **alpha > 0.99).count() >= 3,
        "TownHall keeps its main geosets visible in Stand: {stand_alphas:?}"
    );

    // The terrain atlas composites every BLP alpha mask over the sampled ground
    // colour, so the final atlas is opaque by construction (a transparent texel
    // was the black hole in the first builds). The old assertion expected the
    // raw mask to survive; that contradicted AtlasBuilder::push.
    let atlas = assets.terrain_atlas.as_ref().unwrap();
    let mut transparent = 0usize;
    let mut opaque = 0usize;
    let mut green = 0usize;
    let mut warm = 0usize;
    for px in atlas.rgba.chunks_exact(4) {
        if px[3] == 0 { transparent += 1; }
        if px[3] == 255 { opaque += 1; }
        if px[1] as i32 > px[0] as i32 + 8 && px[1] as i32 > px[2] as i32 + 2 { green += 1; }
        // Correct BLP1 JPEG mapping made Village_Dirt brown/tan; the old
        // inverted conversion turned it teal, so "green only" is no longer a
        // valid invariant for an atlas that also contains dirt paths.
        if px[0] as i32 > px[1] as i32 + 8 && px[1] as i32 > px[2] as i32 + 4 { warm += 1; }
    }
    let total = (atlas.width * atlas.height) as usize;
    assert_eq!(transparent, 0, "atlas is opaque after compositing: {transparent}/{total}");
    assert!(opaque > total / 100, "opaque terrain texels: {opaque}/{total}");
    assert!(
        green + warm > total / 5,
        "terrain atlas has grass+dirt colour: green={green} warm={warm} total={total}"
    );

    // FDF UI engine: real chrome/resources/info/upper-bar definitions and
    // their MPQ textures must be loaded, not a hand-made placeholder HUD.
    assert!(
        assets.ui.doc_count() >= 5,
        "FDF docs loaded: {}",
        assets.ui.doc_count()
    );
    assert!(
        assets.ui.has_root("ConsoleUI") && assets.ui.has_root("ResourceBarFrame"),
        "FDF roots present"
    );
    assert!(
        assets.ui.texture_count() >= 8,
        "UI textures loaded: {}",
        assets.ui.texture_count()
    );
    assert!(
        assets.ui.texture("ConsoleTexture01").is_some(),
        "human console tile resolved through war3skins.txt"
    );
    assert!(
        assets.ui.texture("BTNFootman").is_some(),
        "command-button icon loaded"
    );

    // TownHall bounds must be sane after Z-up conversion (height, not the huge
    // helper plane from the bind pose).
    assert!(townhall.height > 150.0, "TownHall height {}", townhall.height);
}
