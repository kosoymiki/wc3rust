//! Agent C named tests: real BLP icon keys + ghost alpha pipeline.
//!
//! The oracle is the decoded-retail catalog produced from the operator's
//! War3.mpq by the asset pipeline (10,523/10,523 extraction audit, 3,935/3,935
//! BLP decoded). Included at compile time so these tests need no retail MPQ at
//! runtime while still checking against real file names and alpha statistics.

use wc3_assetdb::unit_icons::{
    AUTHORITATIVE_ALPHA_BITS, GHOST_ALPHA, GHOST_BUILDING_TEXTURES, MAGENTA_IS_TRANSPARENCY_KEY,
    UNIT_ICON_KEYS, alpha_is_authoritative, blp_path_for_def, ghost_mesh_alpha,
    icon_key_for_def, icon_key_from_path,
};

const CATALOG: &str = include_str!("../../../analysis/agent-assets/texture-catalog.tsv");

/// One decoded-BLP catalog row: `(alpha_bits, alpha_mean, ok)`.
fn catalog_row(blp_path: &str) -> Option<(u8, u8, bool)> {
    let cols: Vec<&str> = CATALOG
        .lines()
        .find(|line| line.split('\t').next() == Some(blp_path))?
        .split('\t')
        .collect();
    Some((
        cols.get(5)?.parse().ok()?,
        cols.get(9)?.parse().ok()?,
        cols.get(11)? == &"true",
    ))
}

#[test]
#[allow(non_snake_case)]
fn artC_icon_keys() {
    assert!(
        UNIT_ICON_KEYS.len() >= 8,
        "need >=8 verified unit defs, got {}",
        UNIT_ICON_KEYS.len()
    );

    let mut defs = Vec::new();
    let mut keys = Vec::new();
    for entry in UNIT_ICON_KEYS {
        assert!(!defs.contains(&entry.def_id), "duplicate def {}", entry.def_id);
        assert!(!keys.contains(&entry.icon_key), "duplicate key {}", entry.icon_key);
        defs.push(entry.def_id);
        keys.push(entry.icon_key);

        assert!(
            entry.icon_key.starts_with("BTN") && !entry.icon_key.starts_with("DISBTN"),
            "icon key must be an enabled command button: {}",
            entry.icon_key
        );
        let expected = format!(r"ReplaceableTextures\CommandButtons\{}.blp", entry.icon_key);
        assert_eq!(entry.blp_path, expected, "BLP path/key mismatch for {}", entry.def_id);
        assert_eq!(
            icon_key_from_path(entry.blp_path),
            Some(entry.icon_key),
            "basename round-trip failed for {}",
            entry.def_id
        );
        assert_eq!(icon_key_for_def(entry.def_id), Some(entry.icon_key));
        assert_eq!(blp_path_for_def(entry.def_id), Some(entry.blp_path));

        let (alpha_bits, _, ok) = catalog_row(entry.blp_path)
            .unwrap_or_else(|| panic!("{} not in decoded-retail catalog", entry.blp_path));
        assert!(ok, "decode failed for {}", entry.blp_path);
        // Command icons are opaque 64x64 BLP1 palettes (no alpha plane); their
        // magenta surround is a colour key, NOT the building-ghost alpha path.
        assert_eq!(alpha_bits, 0, "unexpected icon alpha plane for {}", entry.def_id);
    }

    // Pin the exact keys Agent B's assets.rs must resolve.
    for (def, key) in [
        ("hfoo", "BTNFootman"),
        ("hpea", "BTNPeasant"),
        ("htow", "BTNTownHall"),
        ("hkee", "BTNKeep"),
        ("hcas", "BTNCastle"),
        ("opeo", "BTNPeon"),
        ("ogru", "BTNGrunt"),
        ("ogre", "BTNGreatHall"),
        ("etol", "BTNTreeOfLife"),
        ("unpl", "BTNNecropolis"),
    ] {
        assert_eq!(icon_key_for_def(def), Some(key), "wrong key for {def}");
    }
}

#[test]
#[allow(non_snake_case)]
fn artC_ghost_alpha() {
    assert_eq!(GHOST_ALPHA, 0.5, "WC3 placement preview is 50% alpha");
    assert!(!MAGENTA_IS_TRANSPARENCY_KEY, "§3.6: alpha plane, not magenta");
    assert!(alpha_is_authoritative(AUTHORITATIVE_ALPHA_BITS));
    assert!(!alpha_is_authoritative(0), "no alpha plane != authoritative");
    assert!(!alpha_is_authoritative(4), "4-bit alpha is not the authored case");

    assert!(!GHOST_BUILDING_TEXTURES.is_empty());
    for tex in GHOST_BUILDING_TEXTURES {
        let (alpha_bits, alpha_mean, ok) = catalog_row(tex.blp_path)
            .unwrap_or_else(|| panic!("{} not in decoded-retail catalog", tex.blp_path));
        assert!(ok, "decode failed for {}", tex.blp_path);
        assert_eq!(alpha_bits, tex.alpha_bits, "alpha_bits drift for {}", tex.blp_path);
        assert_eq!(alpha_bits, 8, "ghost texture must own a real 8-bit alpha plane");
        assert!(alpha_is_authoritative(alpha_bits));
        assert_eq!(alpha_mean, tex.alpha_mean, "alpha_mean drift for {}", tex.blp_path);
        assert!(
            (1..255).contains(&alpha_mean),
            "ghost texture must have real partial transparency, got {alpha_mean}"
        );
    }

    // Mirrors game.rs:7285 alpha * ghost_alpha (mesh + layer alpha).
    assert_eq!(ghost_mesh_alpha(0), 0);
    assert_eq!(ghost_mesh_alpha(255), 127);
    assert_eq!(ghost_mesh_alpha(200), 100);
    assert!(ghost_mesh_alpha(10) <= ghost_mesh_alpha(20));
}
