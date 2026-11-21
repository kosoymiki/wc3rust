//! Verified Warcraft III command-button icon keys and the ghost-alpha contract.
//!
//! Provenance (observed 2026-11-21, operator's private retail data):
//! * `def_id -> icon_key` is read from the extracted object data in
//!   `src/wc3rust/analysis/war3_data` (the `Art=` field of each unit block),
//!   e.g. `[hfoo] Art=ReplaceableTextures\CommandButtons\BTNFootman.blp`.
//! * Every `blp_path` exists in `analysis/war3_mpq_listing.txt` and was fully
//!   decoded from `War3.mpq` by the asset pipeline; its sha256_16 / alpha_bits
//!   / alpha_mean are recorded in `analysis/agent-assets/texture-catalog.tsv`.
//! * The alpha rule is `analysis/agent-hud/CONSOLE_UI_SPEC.md` §10.2 (citing
//!   `GITHUB_UI_RESEARCH.md` §3.6): a paletted BLP1 stores a separate alpha
//!   plane after the index plane. Transparent texels may keep magenta
//!   `#ff00ff` in RGB; the alpha plane is authoritative, never the colour key.
//!
//! This module is data + pure helpers only. It does not touch the game, UI or
//! render crates; Agent B's `assets.rs` consumes [`icon_key_for_def`] and
//! [`blp_path_for_def`] through the public `wc3_assetdb::unit_icons` path.

/// One verified object-editor def -> command-icon mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitIcon {
    /// WC3 object-editor fourCC, e.g. `hfoo` (Footman) or `ogre` (Great Hall).
    pub def_id: &'static str,
    /// Texture-cache key passed to `UiTheme::texture` (e.g. `BTNFootman`).
    pub icon_key: &'static str,
    /// Full MPQ path of the decoded source BLP.
    pub blp_path: &'static str,
}

/// Real, retail-verified def -> icon keys. Ten common starting/building defs:
/// workers, combat units and every race's tier-1/tier-2 hall.
pub const UNIT_ICON_KEYS: &[UnitIcon] = &[
    UnitIcon { def_id: "hfoo", icon_key: "BTNFootman", blp_path: r"ReplaceableTextures\CommandButtons\BTNFootman.blp" },
    UnitIcon { def_id: "hpea", icon_key: "BTNPeasant", blp_path: r"ReplaceableTextures\CommandButtons\BTNPeasant.blp" },
    UnitIcon { def_id: "htow", icon_key: "BTNTownHall", blp_path: r"ReplaceableTextures\CommandButtons\BTNTownHall.blp" },
    UnitIcon { def_id: "hkee", icon_key: "BTNKeep", blp_path: r"ReplaceableTextures\CommandButtons\BTNKeep.blp" },
    UnitIcon { def_id: "hcas", icon_key: "BTNCastle", blp_path: r"ReplaceableTextures\CommandButtons\BTNCastle.blp" },
    UnitIcon { def_id: "opeo", icon_key: "BTNPeon", blp_path: r"ReplaceableTextures\CommandButtons\BTNPeon.blp" },
    UnitIcon { def_id: "ogru", icon_key: "BTNGrunt", blp_path: r"ReplaceableTextures\CommandButtons\BTNGrunt.blp" },
    UnitIcon { def_id: "ogre", icon_key: "BTNGreatHall", blp_path: r"ReplaceableTextures\CommandButtons\BTNGreatHall.blp" },
    UnitIcon { def_id: "etol", icon_key: "BTNTreeOfLife", blp_path: r"ReplaceableTextures\CommandButtons\BTNTreeOfLife.blp" },
    UnitIcon { def_id: "unpl", icon_key: "BTNNecropolis", blp_path: r"ReplaceableTextures\CommandButtons\BTNNecropolis.blp" },
];

/// Icon-cache key for a def id, e.g. `icon_key_for_def("opeo") == "BTNPeon"`.
pub fn icon_key_for_def(def_id: &str) -> Option<&'static str> {
    UNIT_ICON_KEYS
        .iter()
        .find(|entry| entry.def_id == def_id)
        .map(|entry| entry.icon_key)
}

/// Source BLP path for a def id, e.g. `…\CommandButtons\BTNPeon.blp`.
pub fn blp_path_for_def(def_id: &str) -> Option<&'static str> {
    UNIT_ICON_KEYS
        .iter()
        .find(|entry| entry.def_id == def_id)
        .map(|entry| entry.blp_path)
}

/// Extract the cache key from a command-button BLP path.
///
/// `ReplaceableTextures\CommandButtons\BTNFootman.blp` -> `BTNFootman`.
pub fn icon_key_from_path(path: &str) -> Option<&str> {
    let name = path.rsplit(['\\', '/']).next()?;
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    (stem.starts_with("BTN") && !stem.starts_with("DISBTN")).then_some(stem)
}

/// Ghost placement preview multiplies the real model's per-layer alpha by this
/// factor (`crates/wc3-game/src/game.rs:7285`, read-only reference).
pub const GHOST_ALPHA: f32 = 0.5;

/// §3.6 rule: magenta in RGB is a leftover colour, NOT a transparency key when
/// an alpha plane exists. Building/ghost BLPs below carry a real alpha plane.
pub const MAGENTA_IS_TRANSPARENCY_KEY: bool = false;

/// Minimum alpha depth that counts as a real alpha plane rather than an opaque
/// palette (BLP1 supports 0/1/4/8-bit alpha; 8-bit is the fully authored case).
pub const AUTHORITATIVE_ALPHA_BITS: u8 = 8;

/// A building BLP used as a placement-preview (ghost) texture, with the
/// alpha-plane statistics observed in the decoded catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GhostTexture {
    pub blp_path: &'static str,
    pub alpha_bits: u8,
    pub alpha_mean: u8,
}

/// Real building BLPs with an 8-bit alpha plane. The ghost renders these
/// through per-layer mesh alpha scaled by [`GHOST_ALPHA`]; it does not key out
/// magenta texels.
pub const GHOST_BUILDING_TEXTURES: &[GhostTexture] = &[
    GhostTexture {
        blp_path: r"Buildings\Human\TownHall\TownHallCastleKeep.blp",
        alpha_bits: 8,
        alpha_mean: 232,
    },
    GhostTexture {
        blp_path: r"Buildings\Orc\GreatHall\GreatHall.blp",
        alpha_bits: 8,
        alpha_mean: 229,
    },
];

/// True when a BLP carries a real alpha plane that must be used for cutout
/// instead of any magenta colour key.
pub fn alpha_is_authoritative(alpha_bits: u8) -> bool {
    alpha_bits >= AUTHORITATIVE_ALPHA_BITS
}

/// Per-texel ghost alpha: the authored alpha scaled by [`GHOST_ALPHA`].
pub fn ghost_mesh_alpha(base_alpha: u8) -> u8 {
    (f32::from(base_alpha) * GHOST_ALPHA) as u8
}
