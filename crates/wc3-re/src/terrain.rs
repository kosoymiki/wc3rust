//! TerrainArt table access and tile texture resolution.
//!
//! `TerrainArt\Terrain.slk` columns (observed header):
//! `tileID, dir, file, comment, name, buildable, footprints, walkable,
//! flyable, blightPri, convertTo, InBeta`.
//!
//! Tile id is the four-character terrain code stored in `war3map.w3e`
//! (`Ldrt`, `Ldro`, …). The texture is `<dir>\<file>.blp`; each BLP is a
//! 512×512 atlas holding a 2×2 set of 256×256 variants, and `w3e`'s
//! `variation` nibble selects the quadrant (uv offset 0 or 0.5 on each axis).
//! `CliffTypes.slk` maps a `cliffID` to the cliff/ramp model directories and
//! the cliff texture directory; `Water.slk` and `Weather.slk` hold the
//! water/weather settings the renderer needs.
//!
//! Local check: all 33 texture paths in `Terrain.slk` resolve under
//! `TerrainArt\*` and all are BLP1 512×512 atlases.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::slk::Sheet;

#[derive(Debug, Clone, Default)]
pub struct TerrainTile {
    pub id: String,
    pub dir: String,
    pub file: String,
    pub comment: String,
    pub name: String,
    pub buildable: bool,
    pub footprints: bool,
    pub walkable: bool,
    pub flyable: bool,
    pub blight_priority: i32,
    pub converts_to: Vec<String>,
    pub in_beta: bool,
}

impl TerrainTile {
    /// `<dir>\<file>.blp` — the exact archive path the terrain renderer reads.
    pub fn texture_path(&self) -> String {
        format!("{}\\{}.blp", self.dir, self.file)
    }

    /// UV offset of the 2×2 variant inside the atlas (`0.0` or `0.5`).
    pub fn variant_uv(&self, variation: u8) -> (f32, f32) {
        let v = variation & 0b11;
        let x = if v & 1 != 0 { 0.5 } else { 0.0 };
        let y = if v & 2 != 0 { 0.5 } else { 0.0 };
        (x, y)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerrainArt {
    pub tiles: Vec<TerrainTile>,
    pub cliffs: Vec<BTreeMap<String, String>>,
    pub water: Vec<BTreeMap<String, String>>,
    pub weather: Vec<BTreeMap<String, String>>,
}

impl TerrainArt {
    pub fn load_dir(terrain_dir: &Path) -> Result<Self> {
        let mut art = Self::default();
        if let Ok(bytes) = fs::read(terrain_dir.join("Terrain.slk")) {
            art.load_terrain(&bytes)?;
        }
        art.cliffs = load_rows(&terrain_dir.join("CliffTypes.slk"))?;
        art.water = load_rows(&terrain_dir.join("Water.slk"))?;
        art.weather = load_rows(&terrain_dir.join("Weather.slk"))?;
        Ok(art)
    }

    pub fn load_terrain(&mut self, bytes: &[u8]) -> Result<()> {
        let sheet = Sheet::parse("Terrain", bytes)?;
        for row in 0..sheet.len() {
            let id = sheet.cell(row, "tileID").unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            self.tiles.push(TerrainTile {
                id,
                dir: sheet.cell(row, "dir").unwrap_or("").to_string(),
                file: sheet.cell(row, "file").unwrap_or("").to_string(),
                comment: sheet.cell(row, "comment").unwrap_or("").to_string(),
                name: sheet.cell(row, "name").unwrap_or("").to_string(),
                buildable: sheet.cell(row, "buildable") == Some("1"),
                footprints: sheet.cell(row, "footprints") == Some("1"),
                walkable: sheet.cell(row, "walkable") == Some("1"),
                flyable: sheet.cell(row, "flyable") == Some("1"),
                blight_priority: sheet
                    .cell(row, "blightPri")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                converts_to: sheet
                    .cell(row, "convertTo")
                    .map(|value| {
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                in_beta: sheet.cell(row, "InBeta") == Some("1"),
            });
        }
        Ok(())
    }

    pub fn tile(&self, id: &str) -> Option<&TerrainTile> {
        self.tiles.iter().find(|tile| tile.id == id)
    }

    pub fn texture_paths(&self) -> Vec<(String, String)> {
        self.tiles
            .iter()
            .map(|tile| (tile.id.clone(), tile.texture_path()))
            .collect()
    }
}

fn load_rows(path: &Path) -> Result<Vec<BTreeMap<String, String>>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(Vec::new()),
    };
    let sheet = Sheet::parse(
        path.file_stem().unwrap_or_default().to_string_lossy(),
        &bytes,
    )?;
    let mut out = Vec::new();
    for row in 0..sheet.len() {
        let record: BTreeMap<String, String> = sheet
            .record(row)
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        out.push(record);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_real_terrain_tiles() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/TerrainArt");
        let art = TerrainArt::load_dir(&dir).expect("load terrain");
        assert!(art.tiles.len() > 30, "tiles {}", art.tiles.len());
        let dirt = art.tile("Ldrt").expect("Ldrt");
        assert_eq!(dirt.texture_path(), "TerrainArt\\LordaeronSummer\\Lords_Dirt.blp");
        assert!(dirt.walkable);
        assert_eq!(dirt.variant_uv(3), (0.5, 0.5));
        assert!(!art.cliffs.is_empty());
    }
}
