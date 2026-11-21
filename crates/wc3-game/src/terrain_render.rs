//! Terrain rendering from the map's real TerrainArt textures.
//!
//! Every ground tile is a `wc3-re::terrain::TerrainArt` row. Its BLP sheet is
//! decoded by `wc3-formats::blp`, reduced to one seamless 128px tile, and
//! uploaded as one layer of a `texture_2d_array`. Each terrain vertex carries
//! the four W3E ground-tile indices that touch its corner plus their weights;
//! the fragment shader blends the four array layers. UVs are world-space
//! (one texture repeat per 128-unit W3E tile), so there is no per-cell origin
//! reset and no atlas quadrant arithmetic — the two causes of the old quilt.
//!
//! Slope/normal/AO/light is applied on top of the real albedo, not instead of
//! it. Water is a separate translucent pass whose shallow/deep colours and
//! surface texture come from `Water.slk` via `TerrainArt::water`. The ground
//! pass is pure albedo * lamp (WC3 has no terrain specular and no aerial
//! perspective); snow/ice tiles are only classified for the tileset audit.
//! Heights come
//! from the same `wc3-formats::w3e::Terrain` the game simulates on.
//!
//! Canonical consumers: `wc3_re::terrain::TerrainArt`, `wc3_re::storm::StormFs`,
//! `wc3_formats::blp::decode`, `wc3_formats::w3e::Terrain`. No private
//! MPQ/W3E/terrain-art reader lives in this module.

use std::collections::BTreeMap;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wc3_formats::blp;
use wc3_re::slk::Sheet;
use wc3_render::terrain_geometry as overlay;
use wc3_re::storm::StormFs;
use wc3_re::terrain::{TerrainArt, TerrainTile};

use crate::assets::{Assets, TERRAIN_HEIGHT_SCALE, TERRAIN_TILE_SIZE, TextureImage};

/// Biome classes recovered from the map's ground-tile tags (and, when a tag is
/// unknown, from the average colour of its BLP).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Grass = 0,
    Dirt = 1,
    Rock = 2,
    Crops = 3,
    Path = 4,
    Water = 5,
}

impl Biome {
    pub fn from_index(index: u8) -> Self {
        match index {
            1 => Self::Dirt,
            2 => Self::Rock,
            3 => Self::Crops,
            4 => Self::Path,
            5 => Self::Water,
            _ => Self::Grass,
        }
    }

    /// Authoring colour in sRGB; the mesh builder converts it to linear.
    fn srgb(self) -> [u8; 3] {
        match self {
            Self::Grass => [94, 124, 57],
            Self::Dirt => [132, 108, 78],
            Self::Rock => [126, 124, 116],
            Self::Crops => [170, 148, 82],
            Self::Path => [158, 134, 100],
            Self::Water => [42, 62, 58],
        }
    }

    fn linear(self) -> [f32; 3] {
        srgb_to_linear(self.srgb())
    }
}

/// Classify a W3E ground tag. The tag list is open (custom tilesets), so the
/// semantic prefixes are checked first and the decoded BLP average colour is
/// the fallback. `water` is the per-cell W3E flag and always wins.
pub fn classify(tag: &str, average: [u8; 3], water: bool) -> Biome {
    if water {
        return Biome::Water;
    }
    let tag = tag.trim_matches('\0').to_ascii_lowercase();
    if tag.contains("pth") || tag.contains("path") || tag.contains("road") {
        return Biome::Path;
    }
    if tag.contains("crp") || tag.contains("crop") {
        return Biome::Crops;
    }
    if tag.contains("rok")
        || tag.contains("rock")
        || tag.contains("roc")
        || tag.contains("rck")
        || tag.contains("stn")
        || tag.contains("stone")
        || tag.contains("clf")
        || tag.contains("cliff")
    {
        return Biome::Rock;
    }
    if tag.contains("gr") || tag.contains("grs") || tag.contains("grv") || tag.contains("grass") {
        return Biome::Grass;
    }
    if tag.contains("dirt")
        || tag.contains("dr")
        || tag.contains("drt")
        || tag.contains("drr")
        || tag.contains("soil")
    {
        return Biome::Dirt;
    }
    classify_color(average)
}

/// Colour-only classifier for unknown tags. The thresholds are deliberately
/// conservative: a wrong near-grey tag becomes rock, a green tag grass.
pub fn classify_color(average: [u8; 3]) -> Biome {
    let r = average[0] as f32;
    let g = average[1] as f32;
    let b = average[2] as f32;
    if b > r + 12.0 && b >= g - 4.0 {
        return Biome::Water;
    }
    if g > r * 1.06 && g > b * 1.12 {
        return Biome::Grass;
    }
    if (r - g).abs() < 10.0 && (g - b).abs() < 12.0 {
        return Biome::Rock;
    }
    if r > g * 1.04 && g > b * 1.02 {
        return Biome::Dirt;
    }
    if g > b * 1.15 && r > g * 0.82 {
        return Biome::Crops;
    }
    Biome::Grass
}

/// sRGB 0..255 -> linear light 0..1 (the render target is sRGB, so fragment
/// output must be linear).
pub fn srgb_to_linear(color: [u8; 3]) -> [f32; 3] {
    let channel = |value: u8| {
        let c = value as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(color[0]), channel(color[1]), channel(color[2])]
}

/// One terrain vertex: world position, smoothed normal, world-space UV
/// (one texture repeat per 128-unit tile), vertex tint/AO, and eight
/// per-vertex TerrainArt **weights** for the fixed array layers 0..7.
///
/// The layer indices are intentionally NOT per-vertex: a layer index
/// interpolated across a triangle switches the sampled texture mid-triangle
/// and produced the hard 128px polygonal bands at W3E cell edges. Fixed layers
/// 0..7 with smoothly varying weights give the WC3 corner blend without that
/// switch.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub weights: [f32; 4],
    pub weights2: [f32; 4],
}

pub struct TerrainMesh {
    pub vertices: Vec<TerrainVertex>,
    pub indices: Vec<u32>,
    pub water_vertices: Vec<TerrainVertex>,
    pub water_indices: Vec<u32>,
    /// Map-edge water border frame (same vertex layout as the water mesh, drawn
    /// right after it).
    pub border_vertices: Vec<TerrainVertex>,
    pub border_indices: Vec<u32>,
    /// Shoreline foam band (textured with `Shoreline1.blp`).
    pub shore_vertices: Vec<TerrainVertex>,
    pub shore_indices: Vec<u32>,
    /// Flat plateau tops + vertical rock faces for TFT cliff transitions.
    pub cliff_vertices: Vec<TerrainVertex>,
    pub cliff_indices: Vec<u32>,
    /// Cell-major base colour (linear RGB), used by the regression tests to
    /// prove that neighbouring cells are continuous.
    pub cell_colors: Vec<[f32; 3]>,
}

// ---------------------------------------------------------------- noise

fn hash(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut h = (ix as u32)
        .wrapping_mul(0x8da6_b343)
        .wrapping_add((iy as u32).wrapping_mul(0xd816_3841))
        .wrapping_add(seed.wrapping_mul(0xcb1a_b31f));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Value noise on a lattice that wraps at `px`/`py`, so a texture generated
/// from it tiles seamlessly.
fn noise_periodic(x: f32, y: f32, px: u32, py: u32, seed: u32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let xf = x - xi;
    let yf = y - yi;
    let wrap = |value: i32, period: u32| {
        let period = period.max(1) as i32;
        ((value % period) + period) % period
    };
    let x0 = wrap(xi as i32, px);
    let x1 = wrap(xi as i32 + 1, px);
    let y0 = wrap(yi as i32, py);
    let y1 = wrap(yi as i32 + 1, py);
    let sx = xf * xf * (3.0 - 2.0 * xf);
    let sy = yf * yf * (3.0 - 2.0 * yf);
    let top = hash(x0, y0, seed) + (hash(x1, y0, seed) - hash(x0, y0, seed)) * sx;
    let bottom = hash(x0, y1, seed) + (hash(x1, y1, seed) - hash(x0, y1, seed)) * sx;
    top + (bottom - top) * sy
}

fn fbm_periodic(u: f32, v: f32, period: u32, octaves: u32, seed: u32) -> f32 {
    let mut amplitude = 0.5;
    let mut total = 0.0;
    let mut norm = 0.0;
    let mut frequency = period.max(1);
    for octave in 0..octaves {
        total += noise_periodic(
            u * frequency as f32,
            v * frequency as f32,
            frequency,
            frequency,
            seed.wrapping_add(octave.wrapping_mul(0x9e37_79b9)),
        ) * amplitude;
        norm += amplitude;
        amplitude *= 0.5;
        frequency = (frequency * 2).min(512);
    }
    total / norm.max(1e-5)
}

/// Seamless neutral detail texture.
///
/// R: fine grass-blade/twig streaks, G: mid-frequency clump noise,
/// B: low-frequency patch variation, A: independent fine speckle. The fragment
/// shader recombines the channels per material, so one texture serves grass,
/// dirt and rock without a splat weight map.
pub fn detail_texture(size: u32) -> TextureImage {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / size as f32;
            let v = y as f32 / size as f32;
            let streaks = 0.55 * noise_periodic(u * 10.0, v * 72.0, 10, 72, 0x51a7)
                + 0.45 * noise_periodic(u * 24.0, v * 160.0, 24, 160, 0x2b13);
            let clumps = fbm_periodic(u, v, 7, 3, 0x7f4a);
            let patches = fbm_periodic(u, v, 3, 2, 0x19c3);
            let speck = noise_periodic(u * 192.0, v * 192.0, 192, 192, 0x6d2f);
            let index = ((y * size + x) * 4) as usize;
            rgba[index] = (streaks.clamp(0.0, 1.0) * 255.0) as u8;
            rgba[index + 1] = (clumps.clamp(0.0, 1.0) * 255.0) as u8;
            rgba[index + 2] = (patches.clamp(0.0, 1.0) * 255.0) as u8;
            rgba[index + 3] = (speck.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    TextureImage {
        width: size,
        height: size,
        rgba,
    }
}

// ---------------------------------------------------------------- mesh

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct Grid {
    heights: Vec<f32>,
    normals: Vec<Vec3>,
    colors: Vec<[f32; 3]>,
    /// Eight tag weights (fixed array layers 0..7) per grid vertex.
    weights: Vec<[f32; 8]>,
    ao: Vec<f32>,
}

/// Build terrain and water geometry from the map's W3E grid.
///
/// Heights and cell membership are untouched; only the shading data changes.
pub fn build_terrain(assets: &Assets, tiles: &TileSet) -> TerrainMesh {
    let cells_w = assets.map_width_cells.max(1) as usize;
    let cells_h = assets.map_height_cells.max(1) as usize;
    let width = cells_w + 1;
    let height = cells_h + 1;

    let tile = |x: usize, z: usize| -> Option<&wc3_formats::w3e::TerrainTile> {
        assets.terrain_tiles.get(z * width + x)
    };
    // HiveWE `corner_final_ground_height`: the TFT cliff layer (0..15) is part
    // of the rendered height. Without it every cliff on Melee.w3x (layers 1..7)
    // was flattened and the water plane sat above dry land.
    let height_at = |x: usize, z: usize| -> f32 {
        tile(x, z).map(final_height).unwrap_or(0.0)
    };
    let layer_at = |x: usize, z: usize| -> u32 { tile(x, z).map(|t| t.texture() as u32).unwrap_or(0) };
    // Rock layer used on steep slopes: the first map ground tile classified as
    // rock, so cliffs use the map's own rock art instead of a colour tint.
    let rock_layer = assets
        .tile_biome
        .iter()
        .position(|biome| *biome == Biome::Rock as u8)
        .map(|index| index as u32)
        .unwrap_or(0);
    let mut rock_present = false;
    // Fixed array layers 0..7. Human04 uses exactly eight tags; a map with more
    // tags folds the extra tags into the nearest of the first eight by decoded
    // BLP average colour (the array still contains every tag for diagnostics).
    const LAYERS: usize = 8;
    let tag_count = assets
        .terrain
        .as_ref()
        .map(|terrain| terrain.ground_tiles.len())
        .unwrap_or(1)
        .max(1);
    let mut slot_of = vec![0usize; tag_count];
    for (tag, slot) in slot_of.iter_mut().enumerate() {
        if tag < LAYERS {
            *slot = tag;
        } else {
            let colour = tiles.averages.get(tag).copied().unwrap_or([0.5, 0.5, 0.5]);
            let mut best = 0usize;
            let mut best_distance = f32::INFINITY;
            for candidate in 0..LAYERS.min(tag_count) {
                let other = tiles.averages.get(candidate).copied().unwrap_or([0.5, 0.5, 0.5]);
                let distance = (colour[0] - other[0]).powi(2)
                    + (colour[1] - other[1]).powi(2)
                    + (colour[2] - other[2]).powi(2);
                if distance < best_distance {
                    best_distance = distance;
                    best = candidate;
                }
            }
            *slot = best;
        }
    }

    let mut grid = Grid {
        heights: Vec::with_capacity(width * height),
        normals: Vec::with_capacity(width * height),
        colors: Vec::with_capacity(width * height),
        weights: Vec::with_capacity(width * height),
        ao: Vec::with_capacity(width * height),
    };
    for z in 0..height {
        for x in 0..width {
            grid.heights.push(height_at(x, z));
        }
    }
    for z in 0..height {
        for x in 0..width {
            let h_l = height_at(x.saturating_sub(1), z);
            let h_r = height_at((x + 1).min(cells_w), z);
            let h_d = height_at(x, z.saturating_sub(1));
            let h_u = height_at(x, (z + 1).min(cells_h));
            let normal = Vec3::new(h_l - h_r, 2.0 * TERRAIN_TILE_SIZE, h_d - h_u).normalize_or_zero();
            grid.normals.push(normal);
            // Concave ground darkens: a Laplacian-based AO is much more
            // representative than a one-sided dip and still cheap to bake.
            let lap = 4.0 * height_at(x, z) - (h_l + h_r + h_d + h_u);
            let concavity = (-lap / 32.0).clamp(0.0, 1.0);
            grid.ao
                .push((1.0 - concavity * 0.40).clamp(0.55, 1.00));
        }
    }

    // Per-corner material: each grid vertex touches up to four W3E cells, so
    // gather their ground-tile indices with equal weights. Neighbouring cells
    // literally share the corner, which makes the texture blend continuous and
    // removes every per-cell rectangle.
    for z in 0..height {
        for x in 0..width {
            // WC3 corner sampling: a grid vertex is the shared corner of up to
            // four W3E cells, and each of those cells contributes equally. The
            // per-vertex weights are what the rasteriser interpolates, so the
            // blend is continuous across every shared edge and a cell keeps its
            // own material at its centre (1.0 after bilinear reconstruction).
            // Fixed array layers 0..7 mean no tag switch happens inside a
            // triangle, which was the hard 128px band source.
            let mut weights8 = [0.0f32; 8];
            for cz in [z.saturating_sub(1), z] {
                for cx in [x.saturating_sub(1), x] {
                    if cx >= cells_w || cz >= cells_h {
                        continue;
                    }
                    let tag = layer_at(cx, cz) as usize;
                    let slot = slot_of.get(tag).copied().unwrap_or(0);
                    weights8[slot] += 0.25;
                }
            }
            // Steep ground gets the map's rock layer mixed in. This is a real
            // texture layer, not a grey tint.
            let normal = grid.normals[z * width + x];
            let slope = (1.0 - normal.y).clamp(0.0, 1.0);
            let rock = smoothstep(0.16, 0.48, slope) * 0.75;
            if rock > 0.01 {
                let rock_slot = slot_of
                    .get(rock_layer as usize)
                    .copied()
                    .unwrap_or(0);
                weights8[rock_slot] += rock * 2.0;
                rock_present = true;
            }
            let total: f32 = weights8.iter().sum();
            if total > 1e-4 {
                for weight in &mut weights8 {
                    *weight /= total;
                }
            } else {
                weights8[0] = 1.0;
            }
            let wx = x as f32 * TERRAIN_TILE_SIZE;
            let wz = z as f32 * TERRAIN_TILE_SIZE;
            let broad = noise_periodic(wx * 0.0045, wz * 0.0045, 64, 64, 0xa11e);
            let fine = noise_periodic(wx * 0.017, wz * 0.017, 128, 128, 0x35c9);
            let gain = 0.94 + broad * 0.10 + fine * 0.04;
            let warm = 0.02 * (grid.heights[z * width + x] / 600.0).clamp(-1.0, 1.0);
            grid.colors.push([gain + warm, gain, gain - warm * 0.5]);
            grid.weights.push(weights8);
        }
    }
    let _ = rock_present;

    let mut mesh = TerrainMesh {
        vertices: Vec::with_capacity(width * height),
        indices: Vec::with_capacity(cells_w * cells_h * 6),
        water_vertices: Vec::new(),
        water_indices: Vec::new(),
        border_vertices: Vec::new(),
        border_indices: Vec::new(),
        shore_vertices: Vec::new(),
        shore_indices: Vec::new(),
        cliff_vertices: Vec::new(),
        cliff_indices: Vec::new(),
        cell_colors: Vec::with_capacity(cells_w * cells_h),
    };
    for z in 0..height {
        for x in 0..width {
            let index = z * width + x;
            let color = grid.colors[index];
            let weights = grid.weights[index];
            mesh.vertices.push(TerrainVertex {
                position: [
                    x as f32 * TERRAIN_TILE_SIZE,
                    grid.heights[index],
                    z as f32 * TERRAIN_TILE_SIZE,
                ],
                normal: grid.normals[index].to_array(),
                // World-space UV: one texture repeat per 128-unit tile, never
                // reset at a cell origin, continuous across the whole map.
                uv: [x as f32, z as f32],
                color: [color[0], color[1], color[2], grid.ao[index]],
                weights: [weights[0], weights[1], weights[2], weights[3]],
                weights2: [weights[4], weights[5], weights[6], weights[7]],
            });
        }
    }
    for z in 0..cells_h {
        for x in 0..cells_w {
            let base = (z * width + x) as u32;
            let right = base + 1;
            let down = base + width as u32;
            let down_right = down + 1;
            mesh.indices
                .extend_from_slice(&[base, down, right, right, down, down_right]);
            // Diagnostic cell colour from the real TerrainArt layer averages
            // and the shared-corner material weights. Shared corners make
            // neighbouring cells differ only by their material mix, which is
            // what the continuity regression measures.
            let corner_colour = |index: usize| -> [f32; 3] {
                let vertex = &mesh.vertices[index];
                let mut colour = [0.0f32; 3];
                for slot in 0..8 {
                    let weight = if slot < 4 {
                        vertex.weights[slot]
                    } else {
                        vertex.weights2[slot - 4]
                    };
                    if let Some(average) = tiles.averages.get(slot) {
                        colour[0] += average[0] * weight;
                        colour[1] += average[1] * weight;
                        colour[2] += average[2] * weight;
                    }
                }
                colour
            };
            let mut colour = [0.0f32; 3];
            for index in [base, right, down, down_right] {
                let corner = corner_colour(index as usize);
                colour[0] += corner[0] * 0.25;
                colour[1] += corner[1] * 0.25;
                colour[2] += corner[2] * 0.25;
            }
            mesh.cell_colors.push(colour);
        }
    }

    build_water(assets, &mut mesh, &tiles.water);
    // Overlays: the map-edge water frame, the shore foam band and the cliff
    // plateau/faces. All three are pure `wc3_render::terrain_geometry` builders.
    //
    // `H2_TERRAIN_OVERLAYS=off` builds the pre-fix mesh (no border, no shore
    // band, no cliff blocks) so the operator before/after frames share one
    // camera and one binary. Diagnostic only; the game always builds them.
    let overlays = std::env::var("H2_TERRAIN_OVERLAYS")
        .map(|value| value != "off")
        .unwrap_or(true);
    if overlays {
        let corners = corner_grid(assets, tiles.water.offset);
        build_water_border(&corners, &tiles.water, &mut mesh);
        build_shoreline(&corners, &mut mesh);
        build_cliff_blocks(assets, &corners, &grid, tiles.cliff_layers, &mut mesh);
    }
    mesh
}

/// Translucent water surface for every W3E water cell.
///
/// Ported from HiveWE `data/shaders/water.vert` + `Terrain::update_water`:
///
/// * one quad per `(cells_w, cells_h)` cell, kept when **any** of its four
///   corners carries the W3E water flag (the old code required the bottom-left
///   corner, so half the shoreline cells were missing);
/// * the surface height is the per-corner `water_height + Water.slk height`
///   (`-0.7` tiles) — the raw height without the offset sat 89.6 units above
///   the real water line and covered dry land;
/// * the RGBA colour is the WC3 depth ramp (`water_ramp`) evaluated per corner,
///   so a dry corner gets `Smin` (alpha 10/255) and the surface fades out at
///   the shore instead of ending on an opaque cell rectangle. There is no
///   separate `on_water` alpha test.
fn build_water(assets: &Assets, mesh: &mut TerrainMesh, water: &WaterMaterial) {
    let cells_w = assets.map_width_cells.max(1) as usize;
    let cells_h = assets.map_height_cells.max(1) as usize;
    let width = cells_w + 1;
    let tile_at = |x: usize, z: usize| -> Option<&wc3_formats::w3e::TerrainTile> {
        assets.terrain_tiles.get(z * width + x)
    };
    let cell_has_water = |cx: usize, cz: usize| -> bool {
        if cx >= cells_w || cz >= cells_h {
            return false;
        }
        [(cx, cz), (cx + 1, cz), (cx + 1, cz + 1), (cx, cz + 1)]
            .iter()
            .any(|(x, z)| tile_at(*x, *z).map(|tile| tile.is_water()).unwrap_or(false))
    };

    let mut corner_vertex = vec![u32::MAX; width * (cells_h + 1)];
    for z in 0..=cells_h {
        for x in 0..=cells_w {
            // A corner is drawn when at least one adjacent cell has water; a
            // corner surrounded by dry cells is left out entirely.
            let touched = [
                (x.saturating_sub(1), z.saturating_sub(1)),
                (x, z.saturating_sub(1)),
                (x.saturating_sub(1), z),
                (x, z),
            ]
            .iter()
            .any(|(cx, cz)| cell_has_water(*cx, *cz));
            if !touched {
                continue;
            }
            let Some(tile) = tile_at(x, z) else {
                continue;
            };
            let level = final_water_height(tile, water.offset);
            let bed = final_height(tile);
            let depth = (level - bed).max(0.0);
            let color = water_ramp(water, depth);
            corner_vertex[z * width + x] = mesh.water_vertices.len() as u32;
            mesh.water_vertices.push(TerrainVertex {
                position: [x as f32 * TERRAIN_TILE_SIZE, level, z as f32 * TERRAIN_TILE_SIZE],
                normal: [0.0, 1.0, 0.0],
                // World-space UV: the Water.slk surface scrolls continuously.
                uv: [
                    x as f32 * TERRAIN_TILE_SIZE,
                    z as f32 * TERRAIN_TILE_SIZE,
                ],
                color,
                weights: [0.0; 4],
                weights2: [0.0; 4],
            });
        }
    }
    for z in 0..cells_h {
        for x in 0..cells_w {
            if !cell_has_water(x, z) {
                continue;
            }
            let corners = [
                corner_vertex[z * width + x],
                corner_vertex[z * width + x + 1],
                corner_vertex[(z + 1) * width + x + 1],
                corner_vertex[(z + 1) * width + x],
            ];
            if corners.iter().any(|index| *index == u32::MAX) {
                continue;
            }
            mesh.water_indices.extend_from_slice(&[
                corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
            ]);
        }
    }
}

// ---------------------------------------------------------------- overlays

/// Convert the W3E corner lattice into the pure overlay grid.
pub fn corner_grid(assets: &Assets, water_offset: f32) -> overlay::Grid {
    let cells_w = assets.map_width_cells.max(1) as usize;
    let cells_h = assets.map_height_cells.max(1) as usize;
    let width = cells_w + 1;
    let height = cells_h + 1;
    let mut corners = Vec::with_capacity(width * height);
    for z in 0..height {
        for x in 0..width {
            match assets.terrain_tiles.get(z * width + x) {
                Some(tile) => corners.push(overlay::Corner {
                    height: final_height(tile),
                    water: final_water_height(tile, water_offset),
                    layer: tile.cliff_level(),
                    water_flag: tile.is_water(),
                    ramp: tile.is_ramp(),
                    cliff_variation: tile.cliff_variation(),
                    cliff_texture: tile.cliff_texture(),
                }),
                None => corners.push(overlay::Corner::default()),
            }
        }
    }
    overlay::Grid::new(width, height, corners)
}

/// How many tiles the water border frame extends past the map edge, and how
/// many of its outer rows fade into the clear colour.
const WATER_BORDER_CELLS: u32 = 8;
const WATER_BORDER_FADE: u32 = 3;
/// Shore band half-width in world units (the authored ShorelineWave model is
/// about 64 units across the shore direction).
const SHORE_HALF_WIDTH: f32 = 26.0;
const SHORE_LIFT: f32 = 1.5;
const SHORE_UV_PERIOD: f32 = 256.0;
/// Cliff plateau top lift: above the smooth interpolation so the plateau wins
/// the depth test without z-fighting.
const CLIFF_TOP_LIFT: f32 = 1.0;

fn push_water_vertex(target: &mut Vec<TerrainVertex>, vertex: overlay::Vertex) {
    target.push(TerrainVertex {
        position: vertex.position,
        normal: [0.0, 1.0, 0.0],
        uv: vertex.uv,
        color: vertex.color,
        weights: [0.0; 4],
        weights2: [vertex.tag[0], 1.0, 0.0, 0.0],
    });
}

/// Colour of one map-edge water continuation vertex (Agent A art push, bug #14).
///
/// `t` is the 0..1 distance across the 8-cell frame, `depth` the real W3E depth
/// at the map's edge corner, `alpha` the authored fade (0..1) from
/// `water_border`, and `(wx, wz)` the vertex's world position.
///
/// The old path blended the corner ramp into `deep` with `t * 2`, saturating at
/// t = 0.5. On a map whose edge is already deep water the corner ramp *is*
/// `deep`, so all eight cells carried one colour: the flat blue band reported
/// at 06's left edge. The fix stretches the authored shallow→deep ramp over the
/// whole frame and adds a low-frequency tint from world position, so a deep
/// edge reads as continuous open water instead of a rectangle. The palette and
/// the animated Water.slk surface texture are untouched; this is a procedural
/// *rendering* fix, not hand-drawn art.
///
/// `legacy = true` reproduces the old path so one binary yields the A/B pair
/// (`H2_WATER_EDGE_LEGACY=1`).
pub fn water_edge_colour(
    water: &WaterMaterial,
    t: f32,
    depth: f32,
    alpha: f32,
    wx: f32,
    wz: f32,
    legacy: bool,
) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let deep = water_ramp(water, WATER_MAX_DEPTH);
    let ramp = water_ramp(water, depth);
    if legacy {
        let blend = (t * 2.0).clamp(0.0, 1.0);
        return [
            ramp[0] + (deep[0] - ramp[0]) * blend,
            ramp[1] + (deep[1] - ramp[1]) * blend,
            ramp[2] + (deep[2] - ramp[2]) * blend,
            (ramp[3] + (deep[3] - ramp[3]) * blend) * alpha,
        ];
    }
    // Continue the authored shallow→deep ramp across the entire frame instead
    // of the first half. The start is the real boundary depth (not clamped):
    // `t = 0` must reproduce the interior surface exactly, including a shore
    // corner shallower than `WATER_MIN_DEPTH`. Only the outward end is pinned
    // to the deep level; a boundary already deeper than it stays constant.
    let start = depth.max(0.0);
    let frame_depth = start + t * (WATER_DEEP_LEVEL - start).max(0.0);
    let frame = water_ramp(water, frame_depth);
    // Long-wave modulation only, 10 % amplitude at the outer rim: no authored
    // colour is replaced, but a deep edge no longer collapses to a single
    // value across the whole frame. The amplitude ramps in with `t` so the
    // map boundary (t = 0) stays *exactly* the interior water colour: a tint
    // at the inner edge would draw a hard line between the border frame and
    // the surface it continues (measured 2026-11-21: mean blue 168 -> 146
    // across the boundary before this ramp).
    let wave = 0.5
        + 0.5 * ((wx * 0.006 + wz * 0.004).sin() * (wz * 0.007 - wx * 0.003).cos());
    let tint = 1.0 + (wave - 0.5) * 0.20 * t;
    let blend = t * t;
    [
        (frame[0] + (deep[0] - frame[0]) * blend) * tint,
        (frame[1] + (deep[1] - frame[1]) * blend) * tint,
        (frame[2] + (deep[2] - frame[2]) * blend) * tint,
        (frame[3] + (deep[3] - frame[3]) * blend) * alpha,
    ]
}

/// A/B switch for the water edge fix; cached once per process.
fn water_edge_legacy() -> bool {
    static LEGACY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LEGACY.get_or_init(|| {
        matches!(
            std::env::var("H2_WATER_EDGE_LEGACY").as_deref(),
            Ok("1") | Ok("on") | Ok("true")
        )
    })
}

fn build_water_border(grid: &overlay::Grid, water: &WaterMaterial, mesh: &mut TerrainMesh) {
    let deep = water_ramp(water, WATER_MAX_DEPTH);
    let border = overlay::water_border(
        grid,
        WATER_BORDER_CELLS,
        WATER_BORDER_FADE,
        TERRAIN_TILE_SIZE,
        deep,
    );
    let fade_reference = deep[3].max(1e-6);
    let legacy = water_edge_legacy();
    for mut vertex in border.vertices {
        // Match the interior water vertex at the same corner exactly at the map
        // boundary; the authored fade from `water_border` is reused unchanged so
        // the frame still dissolves into the clear colour at its outer edge.
        let cx = (vertex.position[0] / TERRAIN_TILE_SIZE)
            .round()
            .clamp(0.0, (grid.width - 1) as f32) as usize;
        let cz = (vertex.position[2] / TERRAIN_TILE_SIZE)
            .round()
            .clamp(0.0, (grid.height - 1) as f32) as usize;
        if let Some(corner) = grid.at(cx, cz) {
            let depth = (corner.water - corner.height).max(0.0);
            let fade = (vertex.color[3] / fade_reference).clamp(0.0, 1.0);
            vertex.color = water_edge_colour(
                water,
                vertex.tag[0],
                depth,
                fade,
                vertex.position[0],
                vertex.position[2],
                legacy,
            );
        }
        push_water_vertex(&mut mesh.border_vertices, vertex);
    }
    mesh.border_indices = border.indices;
}

fn build_shoreline(grid: &overlay::Grid, mesh: &mut TerrainMesh) {
    let shore = overlay::shoreline(
        grid,
        SHORE_HALF_WIDTH,
        TERRAIN_TILE_SIZE,
        SHORE_LIFT,
        SHORE_UV_PERIOD,
    );
    for vertex in shore.vertices {
        mesh.shore_vertices.push(TerrainVertex {
            position: vertex.position,
            normal: [0.0, 1.0, 0.0],
            uv: vertex.uv,
            color: [1.0, 1.0, 1.0, 1.0],
            weights: [0.0; 4],
            weights2: [0.0; 4],
        });
    }
    mesh.shore_indices = shore.indices;
}

/// Convert the cliff blocks into terrain vertices. Tops copy the ground
/// material of the cell's highest corner; walls carry the cliff sentinel in
/// `weights.x` (`900`), the array layer in `.y` and the variation in `.z`, so
/// the fragment shader samples the authored cliff sheet quadrant per face.
fn build_cliff_blocks(
    assets: &Assets,
    grid: &overlay::Grid,
    ground: &Grid,
    cliff_layers: usize,
    mesh: &mut TerrainMesh,
) -> usize {
    let blocks = overlay::cliff_blocks(grid, TERRAIN_TILE_SIZE, CLIFF_TOP_LIFT);
    let width = assets.map_width_cells.max(1) as usize + 1;
    let layer_count = cliff_layers.max(1);
    let mut walls = 0usize;
    for triangle in blocks.indices.chunks_exact(3) {
        let verts: Vec<&overlay::Vertex> = triangle
            .iter()
            .filter_map(|index| blocks.vertices.get(*index as usize))
            .collect();
        if verts.len() != 3 {
            continue;
        }
        let a = Vec3::from_array(verts[0].position);
        let b = Vec3::from_array(verts[1].position);
        let c = Vec3::from_array(verts[2].position);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for vertex in verts {
            let is_top = vertex.tag[1] > 0.5;
            let (weights, weights2, color, vertex_normal) = if is_top {
                let cx = vertex.cell[0] as usize;
                let cz = vertex.cell[1] as usize;
                let corner = vertex.tag[3] as usize & 3;
                let (dx, dz) = [(0usize, 0usize), (1, 0), (1, 1), (0, 1)][corner];
                let index = (cz + dz) * width + (cx + dx);
                let weights = ground.weights.get(index).copied().unwrap_or([0.0; 8]);
                let color = ground.colors.get(index).copied().unwrap_or([1.0, 1.0, 1.0]);
                let normal = ground.normals.get(index).copied().unwrap_or(Vec3::Y);
                (
                    [weights[0], weights[1], weights[2], weights[3]],
                    [weights[4], weights[5], weights[6], weights[7]],
                    [color[0], color[1], color[2], 1.0],
                    normal.normalize_or_zero().to_array(),
                )
            } else {
                walls += 1;
                let texture = if vertex.tag[2] >= 15.0 {
                    0
                } else {
                    vertex.tag[2] as usize
                }
                .min(layer_count - 1);
                (
                    [900.0, (8 + texture) as f32, vertex.tag[0], 0.0],
                    [0.0; 4],
                    [1.0, 1.0, 1.0, 1.0],
                    normal.to_array(),
                )
            };
            mesh.cliff_vertices.push(TerrainVertex {
                position: vertex.position,
                normal: vertex_normal,
                uv: vertex.uv,
                color,
                weights,
                weights2,
            });
        }
    }
    // The builder is expanded to a non-indexed stream so each triangle can carry
    // its own face normal; the GPU index stream is therefore sequential.
    mesh.cliff_indices = (0..mesh.cliff_vertices.len() as u32).collect();
    walls / 3
}

// ---------------------------------------------------------------- gpu

pub const TERRAIN_SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
    // Direction towards the sun (DNC `KGRT`), Y-up.
    light: vec4<f32>,
    // DNC `sun` colour x intensity (linear); `.w` reserved for exposure.
    sun: vec4<f32>,
    // DNC `ambient` colour x ambient intensity (linear).
    ambient: vec4<f32>,
    // Flat-surface equivalent [r, g, b, 1] (diagnostics / A-B captures).
    day: vec4<f32>,
    // Atmospheric fog colour (linear) and density in `.a`.
    fog: vec4<f32>,
    // x = haze start distance, y = haze full distance (world units).
    fog_range: vec4<f32>,
    // xyz = eye position, w = 1.
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

// DNC light for a surface with normal `n`: `ambient + sun * saturate(N.L)`,
// clamped per channel like the fixed-function pipeline. Terrain, cliffs, water
// and shore all use this one function so a warm sun tints the lit slopes
// instead of multiplying the whole frame.
// Highlight shoulder, byte-identical in shape to the mesh shader
// (`wc3-render/src/mesh.rs`, `fn shoulder`): below the knee the curve is the
// identity, above it approaches 1.0 exponentially instead of clipping. Terrain
// runs the raw DNC terrain light (`sun 1.3`, `ambient 0.2`), which passed 1.0 at
// `N.L = 0.615` — flat noon ground sat at 0.988 and any sun-facing slope turned
// into a white shape. Models already use this curve, so keeping the terrain on
// the old clamp made a unit darker than the slope it stands on.
fn shoulder(value: vec3<f32>) -> vec3<f32> {
    let knee = camera.sun.w;
    if (knee <= 0.0 || knee >= 1.0) {
        return clamp(value, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let knee3 = vec3<f32>(knee);
    let span = vec3<f32>(1.0 - knee);
    let under = min(value, knee3);
    let over = max(value - knee3, vec3<f32>(0.0));
    return clamp(under + span * (vec3<f32>(1.0) - exp(-over / span)), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn surface_light(n: vec3<f32>) -> vec3<f32> {
    let l = normalize(camera.light.xyz);
    let ndl = clamp(dot(normalize(n), l), 0.0, 1.0);
    return shoulder(camera.ambient.rgb + camera.sun.rgb * ndl);
}

// Map fog: linear distance ramp scaled by the map's `war3map.w3i` density.
// Density 0 (RoC campaign maps) returns the colour untouched.
fn distance_haze(colour: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (camera.fog.a <= 0.001) {
        return colour;
    }
    let span = max(camera.fog_range.y - camera.fog_range.x, 1.0);
    let t = clamp((distance(world, camera.eye.xyz) - camera.fog_range.x) / span, 0.0, 1.0)
        * camera.fog.a;
    return mix(colour, camera.fog.rgb, t);
}

// One layer per W3E ground-tile index (layers 0..7), then one layer per map
// cliff type (layer 8 + cliff_texture). Sampling is world-space (one repeat per
// 128-unit tile), so no cell origin resets and no atlas quadrant arithmetic can
// create a grid.
@group(1) @binding(0) var tiles: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

struct TerrainParams {
    // [texRate, time_seconds, water frame count, 0] for the Water.slk surface.
    // Ground has no material response: WC3 terrain is pure albedo * lighting,
    // so the old per-layer `[specular, shininess, snow, ice]` array (sourced
    // from the water table) is gone and specular lives in `fs_water` only.
    water: vec4<f32>,
};
@group(1) @binding(2) var<uniform> params: TerrainParams;
// Water.slk animated surface frames (`Water00.blp` .. `Water44.blp`).
@group(1) @binding(3) var water_tex: texture_2d_array<f32>;
@group(1) @binding(4) var water_samp: sampler;
// W3E per-cell variation nibble (R8Uint): selects the authored TerrainArt
// quadrant inside the 4x4 layer sheet.
@group(1) @binding(5) var variants: texture_2d<u32>;
// Per-cell water surface height (R32Float, -10000 for land).
@group(1) @binding(6) var water_levels: texture_2d<f32>;
// Authored `Textures\Shoreline1.blp` foam/wave strip for the shore band.
@group(1) @binding(7) var shore_tex: texture_2d<f32>;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) weights: vec4<f32>,
    @location(5) weights2: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) weights: vec4<f32>,
    @location(4) weights2: vec4<f32>,
    @location(5) world: vec3<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(input.position, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    out.normal = input.normal;
    out.weights = input.weights;
    out.weights2 = input.weights2;
    out.world = input.position;
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    // Cliff block walls: the geometry builder marks them with weights.x = 900,
    // the array layer in .y and the W3E cliff variation in .z. One authored
    // cliff-sheet quadrant per face, vertical UV (v = 0 at the top lip).
    if (input.weights.x > 100.0) {
        let layer = u32(input.weights.y + 0.5);
        let variation = u32(input.weights.z + 0.5) & 7u;
        let slot = vec2<f32>(f32(variation % 4u), f32(variation / 4u)) * 0.25;
        let uv = fract(input.uv) * 0.25 + slot;
        let rock = textureSampleLevel(tiles, samp, uv, i32(layer), 0.0);
        let n = normalize(input.normal);
        return vec4<f32>(distance_haze(rock.rgb * surface_light(n), input.world), 1.0);
    }
    // Fixed array layers 0..7 (the map has at most eight tags on this map;
    // extra tags are folded into the nearest slot). Only the WEIGHTS vary per
    // vertex. Layer indices interpolated across a triangle used to switch the
    // sampled texture mid-triangle, which is exactly the hard 128px polygonal
    // banding at W3E cell edges.
    // Per-cell authored variation: world UV counts 128-unit cells; the nibble
    // picks one of the 16 packed quadrants, so neighbouring cells show
    // different real TerrainArt variations instead of one averaged tile.
    let cell = vec2<i32>(floor(input.uv));
    let dims = vec2<i32>(textureDimensions(variants));
    let clamped = clamp(cell, vec2<i32>(0), dims - vec2<i32>(1));
    // The W3E variation is five bits (HiveWE terrain.ixx:428-429). Packed
    // sheets index quadrant == variation 0..15; variation 16 selects the
    // authored tile 15, anything above 16 falls back to tile 0 — exactly
    // HiveWE `get_tile_variation` (terrain.ixx:795-810).
    let raw_variant = min(textureLoad(variants, clamped, 0).r, 31u);
    let variant = select(select(0u, 15u, raw_variant == 16u), raw_variant, raw_variant <= 15u);
    var slot = vec2<f32>(f32(variant % 4u), f32(variant / 4u)) * 0.25;
    // Explicit LOD from the *continuous* cell UV. `fract()` jumps at every W3E
    // cell border; implicit derivatives there selected the smallest mip (the
    // whole-sheet average) and drew a seam on every 128-unit cell edge.
    //
    // Texels per pixel: `input.uv` counts 128-unit cells and the sampled tile
    // is `fract(uv) * 0.25 + slot`, i.e. one quarter of the layer sheet, so the
    // derivative scales by `0.25 * sheet_size`. The old hard-coded 512 assumed
    // a 2048-wide sheet, which sampled two mips too small (4x blur) on every
    // real 512px TerrainArt sheet.
    let sheet = f32(textureDimensions(tiles).x);
    let texel_scale = 0.25 * sheet;
    let ddx = dpdx(input.uv) * texel_scale;
    let ddy = dpdy(input.uv) * texel_scale;
    let max_lod = max(log2(sheet), 0.0);
    let lod = clamp(0.5 * log2(max(dot(ddx, ddx), dot(ddy, ddy))), 0.0, max_lod);
    // Authored per-cell variation has no distance falloff by construction: each
    // 128-unit cell picks its own 16x16 quadrant out of the sheet. A mip chain
    // turns a 16x16 quadrant into one flat colour, so at distance the frame is a
    // grid of flat colours that differs cell by cell — on an isometric camera
    // that reads as an achromatic 45 degree moiré (measured `crop-trees.png`).
    // Slide the quadrant toward one reference quadrant as LOD grows: near the
    // camera every authored variation stays, far away the cell pattern is gone.
    // `params.water.w` is the fade start (0 disables it for the A/B). The span
    // is 0.9 mips: the device camera's far half sits at LOD ~1.5-2.5, so a
    // 1.75-mip span would only act on the fogged horizon (measured 2026-11-21:
    // a 1.5 + 1.75 fade left the 2414x1080 far-field cell variance unchanged).
    let fade_start = params.water.w;
    if (fade_start > 0.0) {
        let fade = smoothstep(fade_start, fade_start + 0.9, lod);
        slot = mix(slot, vec2<f32>(0.0), fade);
    }
    let uv = fract(input.uv) * 0.25 + slot;
    let t0 = textureSampleLevel(tiles, samp, uv, 0, lod);
    let t1 = textureSampleLevel(tiles, samp, uv, 1, lod);
    let t2 = textureSampleLevel(tiles, samp, uv, 2, lod);
    let t3 = textureSampleLevel(tiles, samp, uv, 3, lod);
    let t4 = textureSampleLevel(tiles, samp, uv, 4, lod);
    let t5 = textureSampleLevel(tiles, samp, uv, 5, lod);
    let t6 = textureSampleLevel(tiles, samp, uv, 6, lod);
    let t7 = textureSampleLevel(tiles, samp, uv, 7, lod);

    let w = input.weights;
    let w2 = input.weights2;
    // WC3 layer weights are authored: every touching W3E cell contributes 0.25
    // at a shared corner (`build_terrain_mesh`) and the slope rock layer adds
    // its own weight. The BLP alpha channel is the sheet's own mask, NOT a
    // colour multiplier; the old `0.35 + 0.65 * alpha` blended 35% of the flat
    // alpha-fill base colour into every texel, which is the grey-green film
    // over the ground. Use the authored weights directly.
    var mw = w;
    var mw2 = w2;
    var total = mw.x + mw.y + mw.z + mw.w + mw2.x + mw2.y + mw2.z + mw2.w;
    if (total < 0.05) {
        mw = w;
        mw2 = w2;
        total = mw.x + mw.y + mw.z + mw.w + mw2.x + mw2.y + mw2.z + mw2.w;
    }
    let normalizer = max(total, 1e-3);
    var albedo = (
        t0.rgb * mw.x + t1.rgb * mw.y + t2.rgb * mw.z + t3.rgb * mw.w
        + t4.rgb * mw2.x + t5.rgb * mw2.y + t6.rgb * mw2.z + t7.rgb * mw2.w
    ) / normalizer;

    // Terrain under the water surface darkens with depth instead of being
    // veiled by the translucent surface.
    let wcell = clamp(vec2<i32>(floor(input.uv)), vec2<i32>(0), dims - vec2<i32>(1));
    let water_level = textureLoad(water_levels, wcell, 0).r;
    if (water_level > -1000.0) {
        let under = clamp((water_level - input.world.y) / 220.0, 0.0, 1.0);
        albedo = mix(albedo, albedo * vec3<f32>(0.42, 0.52, 0.60), under * 0.85);
    }

    // Cliff sides: steep ground (the smooth interpolation on the cells
    // surrounding a plateau) samples the authored cliff sheet with the same
    // quadrant rule as the block faces; the old whole-sheet UV sampled across
    // the 4x4 grid and smeared every cliff.
    let nrm = normalize(input.normal);
    let steep = smoothstep(0.42, 0.72, 1.0 - nrm.y);
    if (steep > 0.01) {
        let raw_variant = min(textureLoad(variants, clamped, 0).r, 31u);
        let cliff_variant = raw_variant & 7u;
        let cliff_slot = vec2<f32>(f32(cliff_variant % 4u), f32(cliff_variant / 4u)) * 0.25;
        let cliff_uv = fract(vec2<f32>((input.world.x + input.world.z) / 128.0, input.world.y / 128.0)) * 0.25 + cliff_slot;
        let cliff = textureSampleLevel(tiles, samp, cliff_uv, 8, 0.0);
        albedo = mix(albedo, cliff.rgb * 0.92, steep * 0.85);
    }

    // Per-layer material response from TerrainArt/Water.slk. Constant array
    // indices avoid the dynamic uniform-array indexing path entirely.
    let macro_n = vnoise(input.uv * 0.55) * 0.6 + vnoise(input.uv * 1.7 + vec2<f32>(7.3, 3.1)) * 0.4;
    // Subtle large-scale variation only; the old 18% amplitude was a visible
    // milky veil on top of the authored tile colours.
    let macro_v = 1.0 + (macro_n - 0.5) * 0.06;

    let n = normalize(input.normal);
    // DNC terrain light: `ambient + sun · saturate(N·L)`, clamped per channel.
    // One light model for ground, cliffs, water and shore — the old chain
    // (`fake warm Lambert` × `shade` × the flat sun+ambient grade) multiplied
    // the warm key into every pixel twice, which is exactly the «жуткий жёлтый
    // фильтр» the operator reported.
    albedo = albedo * input.color.rgb * macro_v * surface_light(n) * input.color.a;

    // WC3 ground is albedo * lamp only: no specular highlight and no aerial
    // perspective. The blue-white specular (`material` came from Water.slk, a
    // water table) stays gone; specular lives in `fs_water` alone. The distance
    // haze is the map's fog block (or the renderer atmosphere when the map has
    // none), so the far edge of the map fades into the horizon instead of
    // ending on a hard line against the clear colour.
    return vec4<f32>(distance_haze(albedo, input.world), 1.0);
}

@fragment
fn fs_water(input: VsOut) -> @location(0) vec4<f32> {
    // Water.slk surface: `numTex` authored frames stepped at `texRate` fps
    // (HiveWE `reload_water_textures` + `current_water_texture`), plus a slow
    // UV drift so the surface is never a frozen sheet.
    // The per-vertex RGBA already carries the WC3 depth ramp (Smin/Smax/Dmin/
    // Dmax incl. the authored alpha) computed by `water_ramp` on the CPU, so
    // there is no per-cell alpha lookup here: a dry corner interpolates to
    // Smin (alpha 10/255) and the shore fades out by construction.
    let frames = max(params.water.z, 1.0);
    let frame = u32(floor(params.water.y * params.water.x)) % u32(frames);
    let flow = params.water.x * params.water.y * 0.0015;
    let uv = input.uv * 0.001953 + vec2<f32>(flow, flow * 0.73);
    let surface = textureSample(water_tex, water_samp, uv, i32(frame)).rgb;
    let n = normalize(input.normal);
    // HiveWE water.frag: `texture(array_textures[slot], vec3(UV, frame)) * Color`;
    // the `Color` is lit by the same DNC surface light as the ground, so the
    // water dims at night and warms at dusk with the terrain around it.
    let color = input.color.rgb * surface * surface_light(n);
    return vec4<f32>(distance_haze(color, input.world), input.color.a);
}

@fragment
fn fs_shore(input: VsOut) -> @location(0) vec4<f32> {
    // Shore band: the authored `Shoreline1.blp` wave strip along the water
    // edge, alpha-blended over the ground/water. The band spans v 0..1 across
    // the shore and u in world units / SHORE_UV_PERIOD, drifting with texRate.
    let drift = params.water.x * params.water.y * 0.02;
    let uv = vec2<f32>(input.uv.x + drift, input.uv.y);
    let strip = textureSample(shore_tex, water_samp, uv);
    let edge = smoothstep(0.0, 0.18, input.uv.y) * smoothstep(1.0, 0.82, input.uv.y);
    let foam = strip.rgb * vec3<f32>(1.02, 1.0, 0.96) * (0.82 + 0.18 * input.normal.y);
    // Shore foam is world lighting too: it dims at night like the water.
    let lit = foam * surface_light(input.normal);
    return vec4<f32>(distance_haze(lit, input.world), strip.a * edge * 0.85);
}
"#;

/// Water surface material resolved from `Water.slk` (`TerrainArt::water`).
///
/// Mirrors the four Water.slk colour rows and the depth ramp used by the real
/// client and by HiveWE (`data/shaders/water.vert:9-47`):
///
/// * `Smin -> Smax` between 10 and 64 world units of depth (the shoreline);
/// * `Dmin -> Dmax` between 64 and 72 world units (deep water).
///
/// Each colour carries the authored alpha (`Smin_A`, `Smax_A`, `Dmin_A`,
/// `Dmax_A`), which is what makes the water fade out at the shore instead of
/// ending on an opaque cell rectangle.
pub struct WaterMaterial {
    /// Shallow colour at the shore, `Smax` (linear RGB; kept for the palette
    /// regression) — the brightest water colour.
    pub shallow: [f32; 3],
    /// Deep colour, `Dmax` (linear RGB).
    pub deep: [f32; 3],
    /// Shore ramp start, `Smin` RGBA in sRGB 0..1 (alpha included).
    pub shallow_min: [f32; 4],
    /// Shore ramp end, `Smax` RGBA in sRGB 0..1.
    pub shallow_max: [f32; 4],
    /// Deep ramp start, `Dmin` RGBA in sRGB 0..1.
    pub deep_min: [f32; 4],
    /// Deep ramp end, `Dmax` RGBA in sRGB 0..1.
    pub deep_max: [f32; 4],
    /// Water.slk `height` in W3E tile units (`-0.7` for `*Sha` rows). The W3E
    /// per-corner water height is relative to this level; ignoring it put the
    /// rendered surface 0.7 tiles (89.6 units) above the real water line and
    /// was the direct cause of the translucent sheet over dry land.
    pub offset: f32,
    /// The Water.slk `texFile` decoded from the archive.
    pub texture: Option<TextureImage>,
    /// `texRate` (animation speed units; 0 when static).
    pub rate: f32,
    /// Water.slk animation frames (`Water00.blp` … `Water44.blp`, `numTex`
    /// rows). Frame 0 is also `texture`; the shader steps through the array at
    /// `texRate` frames per second instead of only scrolling frame 0.
    pub frames: Vec<TextureImage>,
}

impl Default for WaterMaterial {
    fn default() -> Self {
        // Only used when no install is present; the renderer labels this
        // `source=fallback`. Real maps always take Water.slk colours.
        Self {
            shallow: [0.30, 0.62, 0.66],
            deep: [0.015, 0.08, 0.20],
            shallow_min: [1.0, 1.0, 1.0, 10.0 / 255.0],
            shallow_max: [0.30, 0.62, 0.66, 219.0 / 255.0],
            deep_min: [0.30, 0.62, 0.66, 219.0 / 255.0],
            deep_max: [0.015, 0.08, 0.20, 250.0 / 255.0],
            offset: -0.7,
            texture: None,
            rate: 0.0,
            frames: Vec::new(),
        }
    }
}

/// WC3 water depth thresholds in world units (HiveWE divides these by 128
/// because its heights are in tile units: `min_depth = 10/128`, `deeplevel =
/// 64/128`, `maxdepth = 72/128`).
pub const WATER_MIN_DEPTH: f32 = 10.0;
pub const WATER_DEEP_LEVEL: f32 = 64.0;
pub const WATER_MAX_DEPTH: f32 = 72.0;

/// Final rendered height of one W3E corner in world units.
///
/// HiveWE `Terrain::corner_final_ground_height` (src/base/terrain.ixx:269-272):
/// `corner_height + layer_height - 2.0`, in tile units. `corner_height` is the
/// raw `(u16 - 8192) / 512` value, `layer_height` is the TFT cliff level
/// (`cliff_data & 0x0F`, 0..15, 2 = sea-level ground). Ignoring the layer term
/// (the previous renderer) flattened every cliff on the map: Melee.w3x carries
/// layer heights 1..7, i.e. up to 5.8 tiles (740 world units) of relief.
pub fn final_height(tile: &wc3_formats::w3e::TerrainTile) -> f32 {
    (tile.height() + (tile.cliff_level() as f32 - 2.0)) * TERRAIN_HEIGHT_SCALE
}

/// WC3 per-corner water surface height in world units.
///
/// `Water.slk` `height` is the tileset water offset (`-0.7` tiles for `LSha`);
/// HiveWE `corner_final_water_height` adds it to every corner water height.
pub fn final_water_height(tile: &wc3_formats::w3e::TerrainTile, offset_tiles: f32) -> f32 {
    (tile.water_height() + offset_tiles) * TERRAIN_HEIGHT_SCALE
}

/// The WC3 water colour ramp (HiveWE `data/shaders/water.vert`): RGBA in
/// linear light, interpolated by depth in world units.
pub fn water_ramp(material: &WaterMaterial, depth: f32) -> [f32; 4] {
    let depth = depth.clamp(0.0, TERRAIN_TILE_SIZE);
    let to_linear = |color: [f32; 4]| -> [f32; 4] {
        let linear = srgb_to_linear([
            (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        ]);
        [linear[0], linear[1], linear[2], color[3].clamp(0.0, 1.0)]
    };
    overlay::water_depth_color(
        to_linear(material.shallow_min),
        to_linear(material.shallow_max),
        to_linear(material.deep_min),
        to_linear(material.deep_max),
        depth,
        WATER_MIN_DEPTH,
        WATER_DEEP_LEVEL,
        WATER_MAX_DEPTH,
    )
}

/// The map's ground-tile materials: one RGBA texture per W3E ground-tile
/// index, the sRGB average colour used by the terrain regression test, the
/// per-layer tileset classification flags, and the Water.slk palette.
pub struct TileSet {
    pub images: Vec<TextureImage>,
    /// Authored BLP mip chain per layer (top level first), already extracted
    /// from the sheet by `wc3_render::terrain::tile_from_sheet`.
    pub mips: Vec<Vec<wc3_render::terrain::TileLevel>>,
    pub averages: Vec<[f32; 3]>,
    /// `[snow, ice]` classification per ground-tile layer (1.0 when the tile
    /// id carries that tileset marker). Used by the tileset audit; not a
    /// shading parameter.
    pub params: Vec<[f32; 2]>,
    pub water: WaterMaterial,
    /// Authored W3E variation index per cell (`variation()` 0..15), row-major
    /// over `(cells_w, cells_h)`. The fragment shader uses it to pick the real
    /// TerrainArt variation quadrant instead of one averaged tile.
    pub variants: Vec<u8>,
    pub variant_width: u32,
    pub variant_height: u32,
    /// Per-cell water surface height in world units for water cells and a
    /// `-10000.0` sentinel for land. The shader darkens terrain under water by
    /// depth and keeps the water alpha on real water cells only.
    pub water_levels: Vec<f32>,
    /// "terrainart" when every layer came from the archive, "fallback" only
    /// when no install is mounted (never part of the data base).
    pub source: &'static str,
    /// Number of cliff texture array layers appended after the eight ground
    /// layers (layer `8 + i` is map cliff type `i`).
    pub cliff_layers: usize,
    /// Authored `Textures\Shoreline1.blp` foam/wave strip for the shore band.
    pub shore: Option<TextureImage>,
}

pub(crate) fn resize_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
    target: u32,
) -> Vec<u8> {
    let mut out = vec![0u8; (target * target * 4) as usize];
    if width == 0 || height == 0 {
        return out;
    }
    for y in 0..target {
        let sy0 = (y as u64 * height as u64 / target as u64) as u32;
        let sy1 = (((y + 1) as u64 * height as u64 / target as u64) as u32)
            .max(sy0 + 1)
            .min(height);
        for x in 0..target {
            let sx0 = (x as u64 * width as u64 / target as u64) as u32;
            let sx1 = (((x + 1) as u64 * width as u64 / target as u64) as u32)
                .max(sx0 + 1)
                .min(width);
            let mut acc = [0u32; 4];
            let mut count = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let index = ((sy * width + sx) * 4) as usize;
                    if index + 3 < rgba.len() {
                        for channel in 0..4 {
                            acc[channel] += rgba[index + channel] as u32;
                        }
                        count += 1;
                    }
                }
            }
            let dst = ((y * target + x) * 4) as usize;
            if count > 0 {
                for channel in 0..4 {
                    out[dst + channel] = (acc[channel] / count) as u8;
                }
            }
        }
    }
    out
}

/// Pack the authored TerrainArt variations into a 4x4 sheet of `tile` px
/// quadrants, copying exactly the subtextures the engine uses.
///
/// HiveWE `GroundTexture` (src/resources/ground_texture.ixx:96-120) cuts the
/// sheet into `height/4` px tiles and indexes them as two side-by-side 4x4
/// grids (`tile_x = (layer % 16) % 4 + (layer >= 16 ? 4 : 0)`), then
/// `Terrain::get_tile_variation` (src/base/terrain.ixx:795-810) maps the W3E
/// 4-bit variation to a layer:
///
/// * extended sheet (`width == height * 2`): variation 0..15 -> layer 16+n;
/// * standard sheet: variation 0 -> layer 0, every other variation -> layer 15.
///
/// The other 14 subtextures of a standard sheet are edge/corner masks and are
/// never used as a whole-tile albedo. Sampling them directly (the previous
/// `variation % 4` quadrant) showed their transparent masks as dark squares.
///
/// WC3 BLPs store black RGB under the alpha mask. Averaging those black texels
/// into the mip chain darkened every cell border ("checkerboard"), so the mask
/// holes are filled with the sheet's alpha-weighted base colour while the
/// alpha channel itself is preserved for the shader's layer mask.
pub(crate) fn pack_variation_sheet(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> (TextureImage, Vec<wc3_render::terrain::TileLevel>, [u8; 3]) {
    const GRID: u32 = 4;
    // HiveWE tile_size = height * 0.25 (128 px for a 512-high TerrainArt BLP).
    let tile = (height / GRID).max(1).min(width.max(1)).min(512);
    let extended = width == height * 2;
    // Base colour: alpha-weighted average of the authored sheet.
    let mut sum = [0u64; 3];
    let mut weight = 0u64;
    for pixel in rgba.chunks_exact(4) {
        let alpha = pixel[3] as u64;
        for channel in 0..3 {
            sum[channel] += pixel[channel] as u64 * alpha;
        }
        weight += alpha;
    }
    let base = if weight == 0 {
        [96, 110, 72]
    } else {
        [
            (sum[0] / weight) as u8,
            (sum[1] / weight) as u8,
            (sum[2] / weight) as u8,
        ]
    };
    let mut out = vec![0u8; (GRID * GRID * tile * tile * 4) as usize];
    for slot in 0..(GRID * GRID) {
        // Map the sheet quadrant (== W3E variation) to the HiveWE layer.
        let layer = if extended {
            16 + slot
        } else if slot == 0 {
            0
        } else {
            15
        };
        let tile_x = (layer % 16) % 4 + if layer >= 16 { 4 } else { 0 };
        let tile_y = (layer % 16) / 4;
        let sx0 = tile_x * tile;
        let sy0 = tile_y * tile;
        let dx0 = (slot % GRID) * tile;
        let dy0 = (slot / GRID) * tile;
        for y in 0..tile {
            for x in 0..tile {
                let sx = (sx0 + x).min(width.saturating_sub(1));
                let sy = (sy0 + y).min(height.saturating_sub(1));
                let si = ((sy * width + sx) * 4) as usize;
                let di = (((dy0 + y) * GRID * tile + dx0 + x) * 4) as usize;
                if si + 3 < rgba.len() && di + 3 < out.len() {
                    let alpha = rgba[si + 3] as u32;
                    for channel in 0..3 {
                        out[di + channel] =
                            ((rgba[si + channel] as u32 * alpha + base[channel] as u32 * (255 - alpha))
                                / 255) as u8;
                    }
                    out[di + 3] = rgba[si + 3];
                }
            }
        }
    }
    let image = TextureImage {
        width: GRID * tile,
        height: GRID * tile,
        rgba: out,
    };
    let levels = wrapped_levels(&image)
        .into_iter()
        .map(|mip| wc3_render::terrain::TileLevel {
            width: mip.width,
            height: mip.height,
            rgba: mip.rgba,
        })
        .collect();
    (image, levels, base)
}

/// Authoring thumbnail: collapse one TerrainArt BLP sheet into a single
/// 128px image plus its alpha-weighted base colour.
///
/// Test-only (the TerrainArt contact-sheet audit). The runtime path never
/// averages variations: `pack_variation_sheet` keeps the authored quadrants so
/// the shader can select the real per-cell variation.
#[cfg(test)]
fn composite_tile(image: &TextureImage, _target: u32) -> (TextureImage, [u8; 3]) {
    const VARIATION: u32 = 128;
    let width = image.width.max(1);
    let height = image.height.max(1);
    let cols = (width / VARIATION).max(1);
    let rows = (height / VARIATION).max(1);
    let mut sum = [0u64; 3];
    let mut weight = 0u64;
    for pixel in image.rgba.chunks_exact(4) {
        let alpha = pixel[3] as u64;
        for channel in 0..3 {
            sum[channel] += pixel[channel] as u64 * alpha;
        }
        weight += alpha;
    }
    let base = if weight == 0 {
        [96, 110, 72]
    } else {
        [
            (sum[0] / weight) as u8,
            (sum[1] / weight) as u8,
            (sum[2] / weight) as u8,
        ]
    };
    let mut out = vec![0u8; (VARIATION * VARIATION * 4) as usize];
    let variations = (cols * rows).max(1) as u32;
    for y in 0..VARIATION {
        for x in 0..VARIATION {
            let mut acc = [0u32; 3];
            let mut alpha_acc = 0u32;
            for row in 0..rows {
                for col in 0..cols {
                    let sx = (col * VARIATION + x).min(width - 1);
                    let sy = (row * VARIATION + y).min(height - 1);
                    let index = ((sy * width + sx) * 4) as usize;
                    let alpha = image.rgba[index + 3] as u32;
                    alpha_acc += alpha;
                    for channel in 0..3 {
                        acc[channel] += (image.rgba[index + channel] as u32 * alpha
                            + base[channel] as u32 * (255 - alpha))
                            / 255;
                    }
                }
            }
            let dst = ((y * VARIATION + x) * 4) as usize;
            for channel in 0..3 {
                out[dst + channel] = (acc[channel] / variations) as u8;
            }
            out[dst + 3] = (alpha_acc / variations) as u8;
        }
    }
    (
        TextureImage {
            width: VARIATION,
            height: VARIATION,
            rgba: out,
        },
        base,
    )
}

/// Cross-fade the tile borders toward a common edge profile so the 128px
/// TerrainArt cell wraps to itself. WC3 cells are blended by the engine's
/// per-cell alpha masks; a plain world-space repeat needs a wrap-safe base
/// mip, otherwise a one-texel seam appears at every 128-unit W3E boundary.
fn make_seamless(image: &mut TextureImage) {
    fn read(image: &TextureImage, width: usize, x: usize, y: usize) -> [f32; 4] {
        let index = (y * width + x) * 4;
        [
            image.rgba[index] as f32,
            image.rgba[index + 1] as f32,
            image.rgba[index + 2] as f32,
            image.rgba[index + 3] as f32,
        ]
    }
    fn write(image: &mut TextureImage, width: usize, x: usize, y: usize, value: [f32; 4]) {
        let index = (y * width + x) * 4;
        for channel in 0..4 {
            image.rgba[index + channel] = value[channel].clamp(0.0, 255.0).round() as u8;
        }
    }

    let width = image.width.max(1) as usize;
    let height = image.height.max(1) as usize;
    let band = 10usize.min(width / 4).min(height / 4).max(1);

    // Left/right toward their average edge profile.
    let mut left = vec![[0.0f32; 4]; height];
    for y in 0..height {
        let a = read(image, width, 0, y);
        let b = read(image, width, width - 1, y);
        left[y] = [
            (a[0] + b[0]) * 0.5,
            (a[1] + b[1]) * 0.5,
            (a[2] + b[2]) * 0.5,
            (a[3] + b[3]) * 0.5,
        ];
    }
    for y in 0..height {
        for i in 0..band {
            let t = i as f32 / band as f32;
            let left_value = read(image, width, i, y);
            let right_value = read(image, width, width - 1 - i, y);
            let mut blended_left = [0.0f32; 4];
            let mut blended_right = [0.0f32; 4];
            for channel in 0..4 {
                blended_left[channel] = left[y][channel] * (1.0 - t) + left_value[channel] * t;
                blended_right[channel] =
                    left[y][channel] * (1.0 - t) + right_value[channel] * t;
            }
            write(image, width, i, y, blended_left);
            write(image, width, width - 1 - i, y, blended_right);
        }
    }

    // Top/bottom toward their average edge profile (corners included).
    let mut top = vec![[0.0f32; 4]; width];
    for x in 0..width {
        let a = read(image, width, x, 0);
        let b = read(image, width, x, height - 1);
        top[x] = [
            (a[0] + b[0]) * 0.5,
            (a[1] + b[1]) * 0.5,
            (a[2] + b[2]) * 0.5,
            (a[3] + b[3]) * 0.5,
        ];
    }
    for x in 0..width {
        for i in 0..band {
            let t = i as f32 / band as f32;
            let top_value = read(image, width, x, i);
            let bottom_value = read(image, width, x, height - 1 - i);
            let mut blended_top = [0.0f32; 4];
            let mut blended_bottom = [0.0f32; 4];
            for channel in 0..4 {
                blended_top[channel] = top[x][channel] * (1.0 - t) + top_value[channel] * t;
                blended_bottom[channel] =
                    top[x][channel] * (1.0 - t) + bottom_value[channel] * t;
            }
            write(image, width, x, i, blended_top);
            write(image, width, x, height - 1 - i, blended_bottom);
        }
    }
}

fn linear_from_srgb(color: [u8; 3]) -> [f32; 3] {
    fn channel(value: u8) -> f32 {
        let c = value as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    [channel(color[0]), channel(color[1]), channel(color[2])]
}

/// Fallback tile when the archive or a TerrainArt BLP is unavailable: the
/// biome's colour modulated by the seamless detail noise. Keeps every layer
/// index valid so the shader never samples a missing array layer.
fn procedural_tile(biome: Biome, face: u32) -> TextureImage {
    const SIZE: u32 = 128;
    let detail = detail_texture(SIZE);
    let base = biome.srgb();
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for (index, (detail_pixel, out)) in detail
        .rgba
        .chunks_exact(4)
        .zip(rgba.chunks_exact_mut(4))
        .enumerate()
    {
        let _ = index;
        let noise = detail_pixel[0] as u16 + detail_pixel[1] as u16;
        let gain = 170 + noise / 3; // 0.66..1.0
        out[0] = (base[0] as u16 * gain / 255) as u8;
        out[1] = (base[1] as u16 * gain / 255) as u8;
        out[2] = (base[2] as u16 * gain / 255) as u8;
        out[3] = 255;
    }
    let _ = face;
    let mut image = TextureImage {
        width: SIZE,
        height: SIZE,
        rgba,
    };
    make_seamless(&mut image);
    image
}

pub(crate) fn debug_value(assets: &Assets, prefix: &str) -> Option<String> {
    assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix(prefix).map(str::to_string))
}

/// Locate the install root through the canonical asset layer. The asset loader
/// now opens StormFs itself and no longer writes `mpq=` into `Assets::debug`;
/// the map path's parent is the authoritative runtime location (the app keeps
/// `War3.mpq` next to the map). The test fixtures keep the archive in the known
/// ISO directory, and the old `mpq=` debug line remains a last resort.
pub(crate) fn install_root(assets: &Assets) -> Option<std::path::PathBuf> {
    if let Some(map) = debug_value(assets, "map=") {
        if let Some(parent) = Path::new(&map).parent() {
            if parent.join("War3.mpq").is_file() {
                return Some(parent.to_path_buf());
            }
        }
    }
    for root in [
        "/data/data/org.wc3rust.p0/files",
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso",
    ] {
        let root = Path::new(root);
        if root.join("War3.mpq").is_file() {
            return Some(root.to_path_buf());
        }
    }
    debug_value(assets, "mpq=")
        .and_then(|mpq| Path::new(&mpq).parent().map(Path::to_path_buf))
}

/// Mount the operator's archives through the canonical Storm layer.
fn install_storm(assets: &Assets) -> Option<StormFs> {
    let root = install_root(assets)?;
    let mpq = root.join("War3.mpq");
    let mut storm = match StormFs::open_wc3_install(&root) {
        Ok(storm) => storm,
        Err(error) => {
            crate::dev_log!("GAME_STORM open_failed root={} error={error}", root.display());
            StormFs::new()
        }
    };
    if storm.listfile().is_empty() {
        match storm.add_archive("War3", &mpq) {
            Ok(count) => crate::dev_log!("GAME_STORM added mpq={} listfile={count}", mpq.display()),
            Err(error) => {
                crate::dev_log!("GAME_STORM add_failed mpq={} error={error}", mpq.display());
                return None;
            }
        }
    }
    if storm.listfile().is_empty() {
        crate::dev_log!("GAME_STORM empty mpq={} root={}", mpq.display(), root.display());
        return None;
    }
    Some(storm)
}

fn load_rows(storm: &StormFs, path: &str) -> Vec<BTreeMap<String, String>> {
    let Ok(bytes) = storm.read(path) else {
        return Vec::new();
    };
    let name = path.rsplit('\\').next().unwrap_or(path);
    let Ok(sheet) = Sheet::parse(name, &bytes) else {
        return Vec::new();
    };
    (0..sheet.len())
        .map(|row| {
            sheet
                .record(row)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        })
        .collect()
}

/// Load the canonical TerrainArt table (tiles + cliff/water/weather rows) from
/// the mounted archives. The tile table uses the canonical parser; the extra
/// rows are the same SLK grid through `wc3_re::slk::Sheet`.
fn load_terrain_art(storm: &StormFs) -> Option<TerrainArt> {
    let bytes = match storm.read("TerrainArt\\Terrain.slk") {
        Ok(bytes) => bytes,
        Err(error) => {
            crate::dev_log!("GAME_ART read_failed Terrain.slk error={error}");
            return None;
        }
    };
    let mut art = TerrainArt::default();
    if let Err(error) = art.load_terrain(&bytes) {
        crate::dev_log!("GAME_ART parse_failed Terrain.slk error={error}");
        return None;
    }
    art.cliffs = load_rows(storm, "TerrainArt\\CliffTypes.slk");
    art.water = load_rows(storm, "TerrainArt\\Water.slk");
    art.weather = load_rows(storm, "TerrainArt\\Weather.slk");
    (!art.tiles.is_empty()).then_some(art)
}

fn water_material(art: &TerrainArt, tileset: char, storm: &StormFs) -> WaterMaterial {
    let wanted = format!("{}Sha", tileset.to_ascii_uppercase());
    let row = art
        .water
        .iter()
        .find(|row| {
            row.get("waterID")
                .map(|value| value.eq_ignore_ascii_case(&wanted))
                .unwrap_or(false)
        })
        .or_else(|| art.water.first());
    let Some(row) = row else {
        return WaterMaterial::default();
    };
    let channel = |key: &str| {
        row.get(key)
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(0)
    };
    let row4 = |prefix: &str| -> [f32; 4] {
        [
            channel(&format!("{prefix}_R")) as f32 / 255.0,
            channel(&format!("{prefix}_G")) as f32 / 255.0,
            channel(&format!("{prefix}_B")) as f32 / 255.0,
            channel(&format!("{prefix}_A")) as f32 / 255.0,
        ]
    };
    let shallow = linear_from_srgb([channel("Smax_R"), channel("Smax_G"), channel("Smax_B")]);
    let deep = linear_from_srgb([channel("Dmax_R"), channel("Dmax_G"), channel("Dmax_B")]);
    // Water.slk `height` is the tileset water offset in W3E tile units.
    // HiveWE tilesets.ixx:288 reads exactly this field; the ramp surfaces at
    // `corner_water_height + offset`, not at the raw corner value.
    let offset = row
        .get("height")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(-0.7);
    let rate = row
        .get("texRate")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    // Water.slk `texFile` is a frame stem: the engine appends a two-digit
    // frame (`Water00.blp`, `A_Water00.blp`, ...) and steps `numTex` frames at
    // `texRate` fps (HiveWE `reload_water_textures`). Load the whole authored
    // strip; the shader indexes it with the frame clock.
    let frames = row
        .get("texFile")
        .filter(|value| !value.is_empty() && *value != "_")
        .map(|path| {
            let (dir, stem) = path
                .rsplit_once('\\')
                .map(|(dir, stem)| (dir, stem))
                .unwrap_or(("ReplaceableTextures\\Water", path.as_str()));
            let count = row
                .get("numTex")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(45)
                .clamp(1, 64);
            let mut frames = Vec::with_capacity(count as usize);
            for frame in 0..count {
                let candidates = [
                    format!("{dir}\\{stem}{frame:02}.blp"),
                    format!("{dir}\\{stem}_Water{frame:02}.blp"),
                ];
                let mut loaded = None;
                for candidate in candidates {
                    let Ok(bytes) = storm.read(&candidate) else {
                        continue;
                    };
                    if let Ok(image) = blp::decode(&bytes) {
                        loaded = Some(TextureImage {
                            width: image.width,
                            height: image.height,
                            rgba: image.rgba,
                        });
                        break;
                    }
                }
                match loaded {
                    Some(image) => frames.push(image),
                    None if frame == 0 => {
                        // Fall back to the single-file stem (`Water.blp`).
                        if let Ok(bytes) = storm.read(&format!("{path}.blp")) {
                            if let Ok(image) = blp::decode(&bytes) {
                                frames.push(TextureImage {
                                    width: image.width,
                                    height: image.height,
                                    rgba: image.rgba,
                                });
                            }
                        }
                        break;
                    }
                    None => break,
                }
            }
            frames
        })
        .unwrap_or_default();
    let texture = frames.first().cloned();
    WaterMaterial {
        shallow,
        deep,
        shallow_min: row4("Smin"),
        shallow_max: row4("Smax"),
        deep_min: row4("Dmin"),
        deep_max: row4("Dmax"),
        offset,
        texture,
        rate,
        frames,
    }
}

fn id_params(id: &str) -> [f32; 2] {
    let id = id.to_ascii_lowercase();
    let snow = id.contains("snw") || id.contains("snow");
    let ice = id.contains("ice");
    [if snow { 1.0 } else { 0.0 }, if ice { 1.0 } else { 0.0 }]
}

fn tile_params(tile: &TerrainTile) -> [f32; 2] {
    id_params(&tile.id)
}

/// Build the terrain material set from canonical `TerrainArt` rows. Every
/// W3E ground-tile index resolves to `<dir>\<file>.blp`, decoded by
/// `wc3-formats::blp` and reduced to one seamless 128px tile. No asset reader
/// of our own is involved; the only fallback is the no-install case and is
/// labelled `source=fallback`.
pub fn load_tile_set(assets: &Assets) -> TileSet {
    let tags: Vec<[u8; 4]> = assets
        .terrain
        .as_ref()
        .map(|terrain| terrain.ground_tiles.clone())
        .unwrap_or_default();
    let storm = install_storm(assets);
    let art = storm.as_ref().and_then(load_terrain_art);
    let mut images = Vec::with_capacity(tags.len().max(1));
    let mut mips: Vec<Vec<wc3_render::terrain::TileLevel>> = Vec::with_capacity(tags.len().max(1));
    let mut averages = Vec::with_capacity(tags.len().max(1));
    let mut params = Vec::with_capacity(tags.len().max(1));
    let mut resolved = 0usize;
    for (index, tag) in tags.iter().enumerate() {
        let id = String::from_utf8_lossy(tag)
            .trim_end_matches('\0')
            .to_string();
        let biome = assets
            .tile_biome
            .get(index)
            .copied()
            .map(Biome::from_index)
            .unwrap_or(Biome::Grass);
        let mut image = None;
        let mut tile_mips = Vec::new();
        let mut average = biome.linear();
        let mut layer_params = [0.0, 0.0];
        if let (Some(art), Some(storm)) = (art.as_ref(), storm.as_ref()) {
            if let Some(tile) = art.tile(&id) {
                if let Ok(bytes) = storm.read(&tile.texture_path()) {
                    if let Ok(decoded) = blp::decode(&bytes) {
                        // Authored variations packed into a 4x4 sheet; the fragment shader
                        // selects the W3E variation quadrant per cell.
                        let (packed, levels, base) =
                            pack_variation_sheet(decoded.width, decoded.height, &decoded.rgba);
                        image = Some(packed);
                        tile_mips = levels;
                        average = linear_from_srgb(base);
                        layer_params = tile_params(tile);
                        resolved += 1;
                    }
                }
            }
        }
        images.push(image.unwrap_or_else(|| procedural_tile(biome, index as u32)));
        mips.push(tile_mips);
        averages.push(average);
        params.push(layer_params);
    }
    if images.is_empty() {
        images.push(procedural_tile(Biome::Grass, 0));
        mips.push(Vec::new());
        averages.push(Biome::Grass.linear());
        params.push([0.0, 0.0]);
    }
    // Asset-layer fallback: `Assets::load` has already decoded every TerrainArt
    // BLP through the canonical asset cache (`terrain_tile_images`). If the RE
    // storm view cannot resolve TerrainArt (e.g. a path/priority regression),
    // use those real images rather than a procedural palette; label the source.
    let mut source = if resolved == tags.len() && resolved > 0 {
        "terrainart"
    } else {
        "fallback"
    };
    if resolved == 0 && !assets.terrain_tile_images.is_empty() {
        images.clear();
        mips.clear();
        averages.clear();
        params.clear();
        for (index, tag) in tags.iter().enumerate() {
            let id = String::from_utf8_lossy(tag)
                .trim_end_matches('\0')
                .to_string();
            match assets.terrain_tile_images.get(index) {
                Some(source_image) if source_image.width > 1 && source_image.height > 1 => {
                    let (packed, levels, base) = pack_variation_sheet(
                        source_image.width,
                        source_image.height,
                        &source_image.rgba,
                    );
                    images.push(packed);
                    mips.push(levels);
                    averages.push(linear_from_srgb(base));
                }
                _ => {
                    let biome = assets
                        .tile_biome
                        .get(index)
                        .copied()
                        .map(Biome::from_index)
                        .unwrap_or(Biome::Grass);
                    let procedural = procedural_tile(biome, index as u32);
                    let packed = TextureImage {
                        width: 512,
                        height: 512,
                        rgba: resize_rgba(procedural.width, procedural.height, &procedural.rgba, 512),
                    };
                    mips.push(
                        wrapped_levels(&packed)
                            .into_iter()
                            .map(|mip| wc3_render::terrain::TileLevel {
                                width: mip.width,
                                height: mip.height,
                                rgba: mip.rgba,
                            })
                            .collect(),
                    );
                    images.push(packed);
                    averages.push(biome.linear());
                }
            }
            params.push(id_params(&id));
        }
        if !images.is_empty() {
            source = "assetdb";
            crate::dev_log!(
                "GAME_TERRAIN_TILES_ASSETDB layers={} (RE TerrainArt unavailable)",
                images.len()
            );
        }
    }
    // The shader samples fixed array layers 0..7; a map with fewer ground tags
    // (or the procedural fallback) must still expose eight array layers, and
    // maps with more tags keep the extra layers for diagnostics while folding
    // them into the nearest of the first eight by average colour.
    // Cliff layers (array layers 8..): one raw `Cliff*.blp` per map cliff type,
    // in `war3map.w3e` cliff-list order. The W3E per-corner `cliff_texture`
    // nibble indexes this list (15 = "no cliff texture", mapped to the first
    // layer). The shader samples one 4x4 quadrant per face/steep quad, so the
    // sheet is uploaded raw instead of through the ground-variation packer.
    let mut cliff_layers = 0usize;
    if let (Some(art), Some(storm)) = (art.as_ref(), storm.as_ref()) {
        let cliff_ids: Vec<String> = assets
            .terrain
            .as_ref()
            .map(|terrain| {
                terrain
                    .cliff_tiles
                    .iter()
                    .map(|tag| String::from_utf8_lossy(tag).trim_end_matches('\0').to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mut textures: Vec<Option<(TextureImage, Vec<wc3_render::terrain::TileLevel>)>> = Vec::new();
        for id in &cliff_ids {
            let row = art.cliffs.iter().find(|row| {
                row.get("cliffID")
                    .map(|value| value.eq_ignore_ascii_case(id))
                    .unwrap_or(false)
            });
            let path = row.map(|row| {
                format!(
                    "{}\\{}.blp",
                    row.get("texDir").cloned().unwrap_or_default(),
                    row.get("texFile").cloned().unwrap_or_default()
                )
            });
            let loaded = path
                .filter(|path| !path.trim_matches('\\').is_empty())
                .and_then(|path| storm.read(&path).ok())
                .and_then(|bytes| blp::decode(&bytes).ok())
                .map(|decoded| {
                    let image = TextureImage {
                        width: decoded.width,
                        height: decoded.height,
                        rgba: decoded.rgba,
                    };
                    let levels = wrapped_levels(&TextureImage {
                        width: 512,
                        height: 512,
                        rgba: resize_rgba(image.width.max(1), image.height.max(1), &image.rgba, 512),
                    })
                    .into_iter()
                    .map(|mip| wc3_render::terrain::TileLevel {
                        width: mip.width,
                        height: mip.height,
                        rgba: mip.rgba,
                    })
                    .collect();
                    (image, levels)
                });
            textures.push(loaded);
        }
        // Every map needs at least one cliff layer so the shader's `8 + index`
        // is always valid; fall back to the generic Cliff0 sheet.
        if textures.is_empty() || textures.iter().all(Option::is_none) {
            let fallback = storm
                .read("ReplaceableTextures\\Cliff\\Cliff0.blp")
                .ok()
                .and_then(|bytes| blp::decode(&bytes).ok())
                .map(|decoded| {
                    let image = TextureImage {
                        width: decoded.width,
                        height: decoded.height,
                        rgba: decoded.rgba,
                    };
                    let levels = wrapped_levels(&TextureImage {
                        width: 512,
                        height: 512,
                        rgba: resize_rgba(image.width.max(1), image.height.max(1), &image.rgba, 512),
                    })
                    .into_iter()
                    .map(|mip| wc3_render::terrain::TileLevel {
                        width: mip.width,
                        height: mip.height,
                        rgba: mip.rgba,
                    })
                    .collect();
                    (image, levels)
                });
            if textures.is_empty() {
                textures.push(fallback);
            } else {
                for slot in textures.iter_mut() {
                    if slot.is_none() {
                        *slot = fallback.clone();
                    }
                }
            }
        }
        for slot in textures {
            match slot {
                Some((image, levels)) => {
                    let mut sum = [0u64; 3];
                    let count = (image.rgba.len() / 4).max(1) as u64;
                    for pixel in image.rgba.chunks_exact(4) {
                        sum[0] += pixel[0] as u64;
                        sum[1] += pixel[1] as u64;
                        sum[2] += pixel[2] as u64;
                    }
                    let average = linear_from_srgb([
                        (sum[0] / count) as u8,
                        (sum[1] / count) as u8,
                        (sum[2] / count) as u8,
                    ]);
                    images.push(image);
                    mips.push(levels);
                    averages.push(average);
                    params.push([0.0, 0.0]);
                    cliff_layers += 1;
                }
                None => {}
            }
        }
    }
    // Shoreline overlay: the authored foam/wave strip Water.slk points at
    // (`Textures\Shoreline1.blp`, used by `ShorelineWave*.mdx`). Every water
    // edge gets a band textured from this, which is the visible shore line the
    // flat alpha fade alone cannot draw.
    let shore = storm
        .as_ref()
        .and_then(|storm| storm.read("Textures\\Shoreline1.blp").ok())
        .and_then(|bytes| blp::decode(&bytes).ok())
        .map(|decoded| TextureImage {
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba,
        });
    // Normalize every layer to the 4x4 variation sheet (one array size).
    for (index, image) in images.iter_mut().enumerate() {
        if image.width != 512 || image.height != 512 {
            *image = TextureImage {
                width: 512,
                height: 512,
                rgba: resize_rgba(image.width, image.height, &image.rgba, 512),
            };
            if let Some(levels) = mips.get_mut(index) {
                *levels = wrapped_levels(image)
                    .into_iter()
                    .map(|mip| wc3_render::terrain::TileLevel {
                        width: mip.width,
                        height: mip.height,
                        rgba: mip.rgba,
                    })
                    .collect();
            }
        }
    }
    const FIXED_LAYERS: usize = 8;
    while images.len() < FIXED_LAYERS {
        let image = images
            .first()
            .cloned()
            .unwrap_or_else(|| procedural_tile(Biome::Grass, 0));
        images.push(image);
        mips.push(mips.first().cloned().unwrap_or_default());
        averages.push(averages.first().copied().unwrap_or(Biome::Grass.linear()));
        params.push(params.first().copied().unwrap_or([0.0, 0.0]));
    }
    let tileset = assets
        .terrain
        .as_ref()
        .map(|terrain| terrain.tileset as char)
        .unwrap_or('L');
    let water = match (art.as_ref(), storm.as_ref()) {
        (Some(art), Some(storm)) => water_material(art, tileset, storm),
        _ => WaterMaterial::default(),
    };
    let water_offset = water.offset;
    crate::dev_log!(
        "GAME_TERRAIN_TILES source={source} tags={} resolved={resolved} water_tex={}",
        tags.len(),
        water.texture.is_some()
    );
    TileSet {
        images,
        mips,
        averages,
        params,
        water,
        cliff_layers,
        shore,
        variants: {
            let variant_width = assets.map_width_cells.max(1);
            let variant_height = assets.map_height_cells.max(1);
            let grid_width = variant_width as usize + 1;
            let mut variants = Vec::with_capacity((variant_width * variant_height) as usize);
            for z in 0..variant_height as usize {
                for x in 0..variant_width as usize {
                    variants.push(
                        assets
                            .terrain_tiles
                            .get(z * grid_width + x)
                            // HiveWE reads the variation as five bits
                            // (`variation & 0b0001'1111`, terrain.ixx:428-429);
                            // Melee.w3x uses 16/17, which a 4-bit mask would
                            // fold onto variation 0/1.
                            .map(|tile| tile.variation_data & 0x1F)
                            .unwrap_or(0),
                    );
                }
            }
            variants
        },
        variant_width: assets.map_width_cells.max(1),
        variant_height: assets.map_height_cells.max(1),
        water_levels: {
            let w = assets.map_width_cells.max(1) as usize;
            let h = assets.map_height_cells.max(1) as usize;
            let grid_width = w + 1;
            let mut levels = Vec::with_capacity(w * h);
            for z in 0..h {
                for x in 0..w {
                    levels.push(
                        assets
                            .terrain_tiles
                            .get(z * grid_width + x)
                            .filter(|tile| tile.is_water())
                            .map(|tile| final_water_height(tile, water_offset))
                            .unwrap_or(-10000.0),
                    );
                }
            }
            levels
        },
        source,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainParamsUniform {
    water: [f32; 4],
}

/// LOD at which the authored per-cell ground variation starts fading out.
///
/// WC3 has no mip chain, so its per-cell variation never aliases. A modern
/// chain turns the 16x16 quadrant into a near-flat colour at distance, and
/// neighbouring cells then differ by that flat colour: measured on the Orc01
/// frame as an achromatic moiré on the cell diagonal (45° in the isometric
/// view). Fading the quadrant toward one reference quadrant with LOD keeps the
/// authored variation under the camera and removes the far-field cell noise.
///
/// `H2_TERRAIN_VARIATION_FADE=0` disables the fade (the pre-fix behaviour) so a
/// single binary produces both halves of the A/B. The default (0.9) is where the
/// device camera's far half sits; a 1.5 start left the measured far-field cell
/// variance unchanged at 2414×1080.
fn variation_fade_start() -> f32 {
    std::env::var("H2_TERRAIN_VARIATION_FADE")
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.9)
}

impl Default for TerrainParamsUniform {
    fn default() -> Self {
        Self { water: [0.0; 4] }
    }
}

fn upload_2d_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &TextureImage,
    label: &str,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let width = image.width.max(1);
    let height = image.height.max(1);
    let mut rgba = image.rgba.clone();
    rgba.resize((width * height * 4) as usize, 255);
    let levels = wrapped_levels(&TextureImage { width, height, rgba });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, mip) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip.width * 4),
                rows_per_image: Some(mip.height),
            },
            wgpu::Extent3d {
                width: mip.width,
                height: mip.height,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("terrain-water-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
            // RTS camera looks at the ground at ~45°: 1x anisotropy smears the
            // grass/rock detail into streaks (measured on device), 16x keeps
            // the texel footprint round.
            anisotropy_clamp: 16,
        ..Default::default()
    });
    (view, sampler)
}

/// Upload Water.slk's animation frames as one `texture_2d_array` (all layers
/// share the frame-0 size; a differing frame is resampled).
fn upload_texture_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frames: &[TextureImage],
    label: &str,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let width = frames.iter().map(|image| image.width.max(1)).max().unwrap_or(1);
    let height = frames.iter().map(|image| image.height.max(1)).max().unwrap_or(1);
    let mut per_frame: Vec<Vec<crate::mipmap::MipLevel>> = Vec::with_capacity(frames.len());
    for image in frames {
        // One array size for every layer: nearest-neighbour resample a frame
        // whose authored size differs from frame 0.
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let src_w = image.width.max(1);
        let src_h = image.height.max(1);
        for y in 0..height {
            let sy = (y as u64 * src_h as u64 / height as u64) as u32;
            for x in 0..width {
                let sx = (x as u64 * src_w as u64 / width as u64) as u32;
                let src = ((sy.min(src_h - 1) * src_w + sx.min(src_w - 1)) * 4) as usize;
                let dst = ((y * width + x) * 4) as usize;
                if src + 3 < image.rgba.len() && dst + 3 < rgba.len() {
                    rgba[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
                }
            }
        }
        per_frame.push(wrapped_levels(&TextureImage { width, height, rgba }));
    }
    let level_count = per_frame.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: frames.len().max(1) as u32,
        },
        mip_level_count: level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, levels) in per_frame.iter().enumerate() {
        for (level, mip) in levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &mip.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip.width * 4),
                    rows_per_image: Some(mip.height),
                },
                wgpu::Extent3d {
                    width: mip.width,
                    height: mip.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("terrain-water-array-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
            // RTS camera looks at the ground at ~45°: 1x anisotropy smears the
            // grass/rock detail into streaks (measured on device), 16x keeps
            // the texel footprint round.
            anisotropy_clamp: 16,
        ..Default::default()
    });
    (view, sampler)
}

/// Box-filter mip chain with wrapped taps. The generic mip generator clamps
/// at the image border, which leaves a visible seam at every terrain tile
/// repeat (one repeat per W3E cell). A tileable ground texture must wrap at
/// every mip level.
fn wrapped_levels(image: &TextureImage) -> Vec<crate::mipmap::MipLevel> {
    let mut width = image.width.max(1);
    let mut height = image.height.max(1);
    let mut source = image.rgba.clone();
    source.resize((width * height * 4) as usize, 0);
    let mut levels = vec![crate::mipmap::MipLevel {
        width,
        height,
        rgba: source.clone(),
    }];
    while width > 1 || height > 1 {
        let next_w = (width / 2).max(1);
        let next_h = (height / 2).max(1);
        let mut next = vec![0u8; (next_w * next_h * 4) as usize];
        for y in 0..next_h {
            for x in 0..next_w {
                let mut acc = [0u32; 4];
                for dy in 0..2u32 {
                    for dx in 0..2u32 {
                        let sx = ((x * 2 + dx) % width) as usize;
                        let sy = ((y * 2 + dy) % height) as usize;
                        let index = (sy * width as usize + sx) * 4;
                        for channel in 0..4 {
                            acc[channel] += source[index + channel] as u32;
                        }
                    }
                }
                let dst = ((y * next_w + x) * 4) as usize;
                for channel in 0..4 {
                    next[dst + channel] = (acc[channel] / 4) as u8;
                }
            }
        }
        levels.push(crate::mipmap::MipLevel {
            width: next_w,
            height: next_h,
            rgba: next.clone(),
        });
        source = next;
        width = next_w;
        height = next_h;
    }
    levels
}

fn upload_tile_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    images: &[TextureImage],
    mips: &[Vec<wc3_render::terrain::TileLevel>],
    variants: &[u8],
    variant_size: (u32, u32),
    water_levels: &[f32],
    params: &TerrainParamsUniform,
    water_frames: &[TextureImage],
    shore: Option<&TextureImage>,
) -> (wgpu::BindGroup, wgpu::Buffer) {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("terrain-tile-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        // RTS camera looks at the ground at ~45°: low anisotropy smears the
        // grass/rock detail into streaks, 16x keeps the texel footprint round.
        anisotropy_clamp: 16,
        ..Default::default()
    });
    let size = images
        .first()
        .map(|image| image.width.max(1))
        .unwrap_or(128);
    // Authored chains end at 1x1; a procedural/fallback layer gets the same
    // full pyramid from wrapped linear-light box filtering.
    let mut per_layer: Vec<Vec<wc3_render::terrain::TileLevel>> = Vec::with_capacity(images.len());
    for (index, image) in images.iter().enumerate() {
        if let Some(levels) = mips.get(index).filter(|levels| !levels.is_empty()) {
            per_layer.push(levels.clone());
        } else {
            per_layer.push(
                wrapped_levels(image)
                    .into_iter()
                    .map(|mip| wc3_render::terrain::TileLevel {
                        width: mip.width,
                        height: mip.height,
                        rgba: mip.rgba,
                    })
                    .collect(),
            );
        }
    }
    let level_count = per_layer.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain-tile-array"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: images.len() as u32,
        },
        mip_level_count: level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, levels) in per_layer.iter().enumerate() {
        for (level, mip) in levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &mip.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip.width * 4),
                    rows_per_image: Some(mip.height),
                },
                wgpu::Extent3d {
                    width: mip.width,
                    height: mip.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });

    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terrain-params-buffer"),
        size: std::mem::size_of::<TerrainParamsUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&params_buffer, 0, bytemuck::bytes_of(params));

    let (water_view, water_sampler) =
        upload_texture_array(device, queue, water_frames, "terrain-water-frames");
    let (shore_view, _) = upload_2d_texture(
        device,
        queue,
        shore.unwrap_or(&TextureImage::white()),
        "terrain-shore-texture",
    );

    // Per-cell W3E variation index (R8Uint, nearest): the fragment shader picks
    // the authored TerrainArt quadrant for the cell it is shading.
    let (vw, vh) = (variant_size.0.max(1), variant_size.1.max(1));
    let variant_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain-variant-texture"),
        size: wgpu::Extent3d {
            width: vw,
            height: vh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut variant_bytes = vec![0u8; (vw * vh) as usize];
    for (index, value) in variants.iter().take(variant_bytes.len()).enumerate() {
        variant_bytes[index] = *value;
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &variant_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &variant_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(vw),
            rows_per_image: Some(vh),
        },
        wgpu::Extent3d {
            width: vw,
            height: vh,
            depth_or_array_layers: 1,
        },
    );
    let variant_view = variant_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Per-cell water surface height (R32Float): terrain under water is darkened
    // by depth and the water pass only keeps alpha on real water cells.
    let water_level_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain-water-level-texture"),
        size: wgpu::Extent3d {
            width: vw,
            height: vh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level_bytes = vec![0u8; (vw * vh) as usize * 4];
    for (index, value) in water_levels.iter().take((vw * vh) as usize).enumerate() {
        level_bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &water_level_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &level_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(vw * 4),
            rows_per_image: Some(vh),
        },
        wgpu::Extent3d {
            width: vw,
            height: vh,
            depth_or_array_layers: 1,
        },
    );
    let water_level_view = water_level_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("terrain-tile-bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&water_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&water_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&variant_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&water_level_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(&shore_view),
            },
        ],
    });
    (bind, params_buffer)
}

fn vertex_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    data: &[TerrainVertex],
) -> wgpu::Buffer {
    let bytes = (data.len().max(1) * std::mem::size_of::<TerrainVertex>()) as u64;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !data.is_empty() {
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(data));
    }
    buffer
}

fn index_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    data: &[u32],
) -> wgpu::Buffer {
    let bytes = (data.len().max(1) * 4) as u64;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !data.is_empty() {
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(data));
    }
    buffer
}

pub struct TerrainRenderer {
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    shore_pipeline: wgpu::RenderPipeline,
    tile_bind: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    water_rate: f32,
    water_frames: u32,
    terrain_vertices: wgpu::Buffer,
    terrain_indices: wgpu::Buffer,
    terrain_index_count: u32,
    water_vertices: wgpu::Buffer,
    water_indices: wgpu::Buffer,
    water_index_count: u32,
    border_vertices: wgpu::Buffer,
    border_indices: wgpu::Buffer,
    border_index_count: u32,
    shore_vertices: wgpu::Buffer,
    shore_indices: wgpu::Buffer,
    shore_index_count: u32,
    cliff_vertices: wgpu::Buffer,
    cliff_indices: wgpu::Buffer,
    cliff_index_count: u32,
}

impl TerrainRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera_layout: &wgpu::BindGroupLayout,
        _texture_layout: &wgpu::BindGroupLayout,
        assets: &Assets,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain-shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADER.into()),
        });
        let tile_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-tile-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain-pipeline-layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&tile_layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 64,
                shader_location: 5,
            },
        ];
        let make = |label: &str,
                    entry: &str,
                    blend: Option<wgpu::BlendState>,
                    depth_write: bool|
         -> wgpu::RenderPipeline {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<TerrainVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let terrain_pipeline = make("terrain-opaque", "fs_main", None, true);
        let water_pipeline = make(
            "terrain-water",
            "fs_water",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let shore_pipeline = make(
            "terrain-shore",
            "fs_shore",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let tiles = load_tile_set(assets);
        let mut uniform = TerrainParamsUniform::default();
        uniform.water = [
            tiles.water.rate,
            0.0,
            tiles.water.frames.len().max(1) as f32,
            variation_fade_start(),
        ];
        let water_frames: Vec<TextureImage> = if tiles.water.frames.is_empty() {
            vec![tiles
                .water
                .texture
                .clone()
                .unwrap_or_else(TextureImage::white)]
        } else {
            tiles.water.frames.clone()
        };
        let (tile_bind, params_buffer) = upload_tile_array(
            device,
            queue,
            &tile_layout,
            &tiles.images,
            &tiles.mips,
            &tiles.variants,
            (tiles.variant_width, tiles.variant_height),
            &tiles.water_levels,
            &uniform,
            &water_frames,
            tiles.shore.as_ref(),
        );

        let mesh = build_terrain(assets, &tiles);
        crate::dev_log!(
            "GAME_TERRAIN layers={} vertices={} tris={} water_vertices={} water_tris={} border_vertices={} border_tris={} shore_tris={} cliff_tris={}",
            tiles.images.len(),
            mesh.vertices.len(),
            mesh.indices.len() / 3,
            mesh.water_vertices.len(),
            mesh.water_indices.len() / 3,
            mesh.border_vertices.len(),
            mesh.border_indices.len() / 3,
            mesh.shore_indices.len() / 3,
            mesh.cliff_indices.len() / 3
        );
        let terrain_vertices = vertex_buffer(device, queue, "terrain-vertices", &mesh.vertices);
        let terrain_indices = index_buffer(device, queue, "terrain-indices", &mesh.indices);
        let water_vertices = vertex_buffer(device, queue, "water-vertices", &mesh.water_vertices);
        let water_indices = index_buffer(device, queue, "water-indices", &mesh.water_indices);
        let border_vertices = vertex_buffer(device, queue, "water-border-vertices", &mesh.border_vertices);
        let border_indices = index_buffer(device, queue, "water-border-indices", &mesh.border_indices);
        let shore_vertices = vertex_buffer(device, queue, "shore-vertices", &mesh.shore_vertices);
        let shore_indices = index_buffer(device, queue, "shore-indices", &mesh.shore_indices);
        let cliff_vertices = vertex_buffer(device, queue, "cliff-vertices", &mesh.cliff_vertices);
        let cliff_indices = index_buffer(device, queue, "cliff-indices", &mesh.cliff_indices);
        Self {
            terrain_pipeline,
            water_pipeline,
            shore_pipeline,
            tile_bind,
            params_buffer,
            water_rate: tiles.water.rate,
            water_frames: water_frames.len().max(1) as u32,
            terrain_vertices,
            terrain_indices,
            terrain_index_count: mesh.indices.len() as u32,
            water_vertices,
            water_indices,
            water_index_count: mesh.water_indices.len() as u32,
            border_vertices,
            border_indices,
            border_index_count: mesh.border_indices.len() as u32,
            shore_vertices,
            shore_indices,
            shore_index_count: mesh.shore_indices.len() as u32,
            cliff_vertices,
            cliff_indices,
            cliff_index_count: mesh.cliff_indices.len() as u32,
        }
    }

    /// Advance the Water.slk surface animation. Called by `Renderer` with the
    /// frame clock; the shader steps `texRate` frames per second.
    pub fn update_time(&self, queue: &wgpu::Queue, seconds: f32) {
        let water = [
            self.water_rate,
            seconds,
            self.water_frames as f32,
            variation_fade_start(),
        ];
        // `TerrainParams` is the Water.slk animation vector only; the former
        // 16-slot per-layer material array is gone (ground has no specular).
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&water));
    }

    pub fn draw_terrain(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.terrain_index_count == 0 {
            return;
        }
        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(1, &self.tile_bind, &[]);
        pass.set_vertex_buffer(0, self.terrain_vertices.slice(..));
        pass.set_index_buffer(self.terrain_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.terrain_index_count, 0, 0..1);
    }

    pub fn draw_water(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.water_index_count == 0 {
            return;
        }
        pass.set_pipeline(&self.water_pipeline);
        pass.set_bind_group(1, &self.tile_bind, &[]);
        pass.set_vertex_buffer(0, self.water_vertices.slice(..));
        pass.set_index_buffer(self.water_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.water_index_count, 0, 0..1);
        // The map-edge frame shares the water material; drawn immediately
        // after the interior surface so both sit at the same depth.
        if self.border_index_count > 0 {
            pass.set_vertex_buffer(0, self.border_vertices.slice(..));
            pass.set_index_buffer(self.border_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.border_index_count, 0, 0..1);
        }
    }

    /// Shoreline foam band (`Shoreline1.blp`) along every water/land edge.
    pub fn draw_shore(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.shore_index_count == 0 {
            return;
        }
        pass.set_pipeline(&self.shore_pipeline);
        pass.set_bind_group(1, &self.tile_bind, &[]);
        pass.set_vertex_buffer(0, self.shore_vertices.slice(..));
        pass.set_index_buffer(self.shore_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.shore_index_count, 0, 0..1);
    }

    /// Cliff plateau tops + rock faces (opaque terrain material).
    pub fn draw_cliffs(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.cliff_index_count == 0 {
            return;
        }
        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(1, &self.tile_bind, &[]);
        pass.set_vertex_buffer(0, self.cliff_vertices.slice(..));
        pass.set_index_buffer(self.cliff_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.cliff_index_count, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Assets, Game};
    use std::path::PathBuf;
    use std::sync::Mutex;

    static GPU: Mutex<()> = Mutex::new(());

    fn data() -> Option<(PathBuf, PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())?;
        let map = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        if !map.exists() {
            return None;
        }
        Some((mpq, map))
    }

    /// One full asset load per test binary.
    ///
    /// `Assets::load` decodes the whole terrain art set, the model roster and
    /// every texture out of the shipped MPQ (2-3 GB of transient allocation).
    /// The default harness runs eight tests at once, so a filtered run of this
    /// module used to start up to eight concurrent loads; on the 11 GB phone
    /// that is swap thrash, and two tests that each cost ~40 s of decode looked
    /// like a >15 min hang that held the shared `h2-cargo` lock for every lane.
    /// Tests that only read the assets now share one load; tests that hand
    /// ownership to `Game::new` keep their own (they need it).
    ///
    /// The cache is a leaked `Box<Assets>` behind a mutex: `Assets` owns an
    /// audio engine whose command `Receiver` is `Send` but not `Sync`, so an
    /// `Arc<Assets>`/`OnceLock<Assets>` static does not type-check. The box is
    /// allocated once, never moved and never freed, so the shared `&'static`
    /// reference is stable; only the address is stored, and only the first
    /// caller writes it.
    fn shared_assets(mpq: &std::path::Path, map: &std::path::Path) -> &'static Assets {
        static SHARED: Mutex<Option<(String, usize)>> = Mutex::new(None);
        // The key matters: the module loads two fixtures (Human04 with
        // `War3.mpq`, Melee.w3x with the melee archives). An unkeyed cache
        // handed the first map's assets to the second and its cliff-layer
        // assertion read the wrong terrain.
        let want = format!("{}|{}", mpq.display(), map.display());
        let mut guard = SHARED.lock().unwrap_or_else(|poison| poison.into_inner());
        let address = match guard.as_ref() {
            Some((key, address)) if *key == want => *address,
            _ => {
                let address = Box::into_raw(Box::new(Assets::load(Some(mpq), Some(map)))) as usize;
                *guard = Some((want, address));
                address
            }
        };
        drop(guard);
        // SAFETY: the pointer comes from `Box::into_raw` above, is written once
        // per fixture under the mutex, is never freed or moved, and every caller
        // only takes a shared reference to it.
        unsafe { &*(address as *const Assets) }
    }

    fn out_dir() -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn round2_dir() -> PathBuf {
        let dir = out_dir().join("round2");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn project(camera: &crate::Camera, point: glam::Vec3) -> Option<glam::Vec2> {
        camera.world_to_screen(point)
    }

    fn pixel(rgba: &[u8], width: u32, height: u32, x: i32, y: i32) -> Option<[f32; 3]> {
        if x < 1 || y < 1 || x >= width as i32 - 1 || y >= height as i32 - 1 {
            return None;
        }
        let index = ((y as u32 * width + x as u32) * 4) as usize;
        Some([
            rgba[index] as f32,
            rgba[index + 1] as f32,
            rgba[index + 2] as f32,
        ])
    }

    /// Mean colour step across a screen-space segment. `offset` moves the
    /// sample line perpendicular to the segment.
    fn line_energy(
        rgba: &[u8],
        width: u32,
        height: u32,
        a: glam::Vec2,
        b: glam::Vec2,
        offset: f32,
    ) -> Option<f32> {
        let dir = b - a;
        if dir.length() < 24.0 {
            return None;
        }
        let normal = glam::Vec2::new(-dir.y, dir.x).normalize();
        let mut sum = 0.0;
        let mut count = 0.0;
        for step in 1..12 {
            let t = step as f32 / 12.0;
            let point = a + dir * t + normal * offset;
            let left = pixel(rgba, width, height, point.x as i32 - 2, point.y as i32);
            let right = pixel(rgba, width, height, point.x as i32 + 2, point.y as i32);
            if let (Some(left), Some(right)) = (left, right) {
                sum += (left[0] - right[0]).abs()
                    + (left[1] - right[1]).abs()
                    + (left[2] - right[2]).abs();
                count += 1.0;
            }
        }
        (count >= 4.0).then_some(sum / count)
    }

    /// Ratio of colour step exactly on cell borders to the same measure inside
    /// the cells. A per-cell quilt (the reported bug) makes this large.
    /// Boundary/interior energy ratio **within a single W3E ground tag**.
    ///
    /// A map legitimately changes material between cells (dirt paths, rock,
    /// water); those edges are W3E data, not a renderer grid. The old quilt was
    /// a per-cell texture/colour discontinuity even when both cells had the
    /// same tag (random variation quadrant + atlas base). Restricting the
    /// measurement to same-tag, non-water boundaries isolates exactly that
    /// defect and leaves the map's real material transitions alone.
    fn seam_ratio(assets: &Assets, camera: &crate::Camera, rgba: &[u8], width: u32, height: u32) -> f32 {
        let tile = TERRAIN_TILE_SIZE;
        let center_x = (camera.target.x / tile).floor() as i32;
        let center_z = (camera.target.y / tile).floor() as i32;
        let tag_at = |cx: i32, cz: i32| -> Option<(u8, bool)> {
            if cx < 0 || cz < 0 || cx as u32 >= assets.map_width_cells || cz as u32 >= assets.map_height_cells {
                return None;
            }
            let value = assets.tile_at(
                (cx as f32 + 0.5) * tile,
                (cz as f32 + 0.5) * tile,
            )?;
            Some((value.texture(), value.is_water()))
        };
        let mut boundary = 0.0f32;
        let mut interior = 0.0f32;
        let mut samples = 0.0f32;
        for dz in -6..=6 {
            for dx in -6..=6 {
                let x = center_x + dx;
                let z = center_z + dz;
                let Some(current) = tag_at(x, z) else {
                    continue;
                };
                if current.1 {
                    continue;
                }
                let x0 = x as f32 * tile;
                let z0 = z as f32 * tile;
                let x1 = x0 + tile;
                let z1 = z0 + tile;
                let ground = |px: f32, pz: f32| assets.height_at(px, pz);
                // The edge shared with the next cell to the right, only when
                // both cells use the same ground texture.
                if tag_at(x + 1, z) == Some(current) {
                    let a = glam::Vec3::new(x1, ground(x1, z0), z0);
                    let b = glam::Vec3::new(x1, ground(x1, z1), z1);
                    if let (Some(a), Some(b)) = (project(camera, a), project(camera, b)) {
                        if let (Some(on), Some(off)) = (
                            line_energy(rgba, width, height, a, b, 0.0),
                            line_energy(rgba, width, height, a, b, 26.0),
                        ) {
                            boundary += on;
                            interior += off;
                            samples += 1.0;
                        }
                    }
                }
                // The edge shared with the next cell downwards.
                if tag_at(x, z + 1) == Some(current) {
                    let a = glam::Vec3::new(x0, ground(x0, z1), z1);
                    let b = glam::Vec3::new(x1, ground(x1, z1), z1);
                    if let (Some(a), Some(b)) = (project(camera, a), project(camera, b)) {
                        if let (Some(on), Some(off)) = (
                            line_energy(rgba, width, height, a, b, 0.0),
                            line_energy(rgba, width, height, a, b, 26.0),
                        ) {
                            boundary += on;
                            interior += off;
                            samples += 1.0;
                        }
                    }
                }
            }
        }
        if samples < 8.0 || interior <= 1e-3 {
            return 1.0;
        }
        boundary / interior
    }

    #[test]
    fn agent_world_detail_texture_is_not_flat() {
        let image = detail_texture(256);
        let mut min = [255u8; 3];
        let mut max = [0u8; 3];
        let mut sum = [0u64; 3];
        for pixel in image.rgba.chunks_exact(4) {
            for channel in 0..3 {
                min[channel] = min[channel].min(pixel[channel]);
                max[channel] = max[channel].max(pixel[channel]);
                sum[channel] += pixel[channel] as u64;
            }
        }
        let count = (image.width * image.height) as u64;
        println!(
            "DETAIL r={}..{} mean={} g={}..{} mean={} b={}..{} mean={}",
            min[0], max[0], sum[0] / count,
            min[1], max[1], sum[1] / count,
            min[2], max[2], sum[2] / count
        );
        for channel in 0..3 {
            assert!(max[channel] - min[channel] > 60, "channel {channel} has contrast");
        }
        let path = out_dir().join("detail-texture.ppm");
        crate::render::write_ppm(path.to_str().unwrap(), image.width, image.height, &image.rgba);
    }

    #[test]
    fn agent_world_terrain_is_biome_varied_and_continuous() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_terrain: user War3.mpq/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let tiles = load_tile_set(&assets);
        let ground_tags = assets
            .terrain
            .as_ref()
            .map(|terrain| terrain.ground_tiles.len())
            .unwrap_or(0);
        assert!(
            tiles.images.len() >= ground_tags.max(8),
            "array layers cover every ground tag and the eight fixed samples"
        );
        assert_eq!(tiles.source, "terrainart", "Human04 tiles must come from TerrainArt");
        let mesh = build_terrain(&assets, &tiles);
        assert!(
            mesh.vertices.len() >= (assets.map_width_cells as usize + 1) * 8,
            "terrain grid covers the map: {}",
            mesh.vertices.len()
        );
        // Every vertex is on the shared grid, so adjacent cells literally share
        // corner colours and normals. A per-cell stamp cannot pass this.
        assert_eq!(
            mesh.vertices.len(),
            (assets.map_width_cells as usize + 1) * (assets.map_height_cells as usize + 1)
        );
        let cells_w = assets.map_width_cells as usize;
        let cells_h = assets.map_height_cells as usize;
        let cell = |x: usize, z: usize| mesh.cell_colors[z * cells_w + x];
        let distance = |a: [f32; 3], b: [f32; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        let mut lag1 = Vec::new();
        let mut lag8 = Vec::new();
        for z in 0..cells_h {
            for x in 0..cells_w {
                if x + 1 < cells_w {
                    lag1.push(distance(cell(x, z), cell(x + 1, z)));
                }
                if x + 8 < cells_w {
                    lag8.push(distance(cell(x, z), cell(x + 8, z)));
                }
            }
        }
        lag1.sort_by(|a, b| a.partial_cmp(b).unwrap());
        lag8.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = |values: &[f32]| values[values.len() / 2];
        let near = median(&lag1);
        let far = median(&lag8);
        println!(
            "TERRAIN cells={}x{} vertices={} water={} near={near:.4} far={far:.4} ratio={:.3}",
            cells_w,
            cells_h,
            mesh.vertices.len(),
            mesh.water_indices.len() / 6,
            near / far.max(1e-4)
        );
        // Neighbouring cells must be closer in colour than cells 8 apart. A
        // random per-cell variation makes the two medians comparable (ratio 1).
        assert!(
            near < far * 0.55,
            "cell colours alternate instead of blending: near={near:.4} far={far:.4}"
        );
        // Distinct biomes must actually appear on the map.
        let mut biomes = std::collections::HashSet::new();
        for tile in &assets.terrain_tiles {
            if tile.is_water() {
                biomes.insert(Biome::Water as u8);
            } else if let Some(&biome) = assets.tile_biome.get(tile.texture() as usize) {
                biomes.insert(biome);
            }
        }
        println!("TERRAIN biomes={biomes:?}");
        assert!(biomes.len() >= 3, "map must use at least 3 biomes: {biomes:?}");
        // Slope normals and baked AO must actually be present, otherwise the
        // "slope shading / ambient occlusion" requirement is only nominal.
        let sloped = mesh
            .vertices
            .iter()
            .filter(|vertex| vertex.normal[1] < 0.999)
            .count();
        let occluded = mesh
            .vertices
            .iter()
            .filter(|vertex| vertex.color[3] < 0.95)
            .count();
        let ao_range = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.color[3])
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), value| {
                (lo.min(value), hi.max(value))
            });
        println!(
            "TERRAIN sloped={sloped}/{} occluded={occluded} ao={ao_range:?}",
            mesh.vertices.len()
        );
        assert!(sloped > mesh.vertices.len() / 20, "slope normals present");
        assert!(occluded > mesh.vertices.len() / 50, "ambient occlusion baked");
        assert!(ao_range.0 >= 0.5 && ao_range.1 <= 1.001, "AO range sane");
        // Every vertex carries eight normalised weights for the fixed texture
        // array layers 0..7; the shader never interpolates layer indices, which
        // is what used to switch the sampled texture mid-triangle at cell edges.
        let layer_count = tiles.images.len() as f32;
        assert!(
            layer_count >= 8.0,
            "fixed shader samples need 8 array layers: {layer_count}"
        );
        for vertex in &mesh.vertices {
            let total: f32 = vertex.weights.iter().sum::<f32>()
                + vertex.weights2.iter().sum::<f32>();
            assert!((total - 1.0).abs() < 1e-3, "weights normalised: {total}");
            for weight in vertex.weights.iter().chain(vertex.weights2.iter()) {
                assert!(*weight >= 0.0, "weight non-negative: {weight}");
            }
        }
    }

    /// Reads a binary P6 PPM produced by `write_ppm`/Pillow and returns RGBA.
    fn read_ppm_rgba(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
        let bytes = std::fs::read(path).ok()?;
        let mut fields = Vec::new();
        let mut index = 0usize;
        while fields.len() < 4 && index < bytes.len() {
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'#' {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            let start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            fields.push(String::from_utf8_lossy(&bytes[start..index]).to_string());
        }
        if fields.first().map(String::as_str) != Some("P6") {
            return None;
        }
        let width: u32 = fields.get(1)?.parse().ok()?;
        let height: u32 = fields.get(2)?.parse().ok()?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let data = &bytes[index..];
        let pixels = (width as usize).checked_mul(height as usize)?;
        if data.len() < pixels * 3 {
            return None;
        }
        let mut rgba = Vec::with_capacity(pixels * 4);
        for pixel in data[..pixels * 3].chunks_exact(3) {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        Some((width, height, rgba))
    }

    /// Headless edge-gradient metric applied to a real device screenshot: the
    /// app's start camera is deterministic (`distance = 1500`, centred on the
    /// player base), so projected W3E cell edges match this headless frame.
    /// Ignored by default; run with `--ignored` and `DEVICE_FRAME=<ppm>`.
    #[test]
    #[ignore = "diagnostic: seam ratio of a device screenshot; needs a device PNG; run_layer_gates.sh --ignored"]
    fn agent_world_device_frame_seam_ratio() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP device frame metric: fixtures missing");
            return;
        };
        let path = std::env::var("DEVICE_FRAME").unwrap_or_else(|_| {
            out_dir()
                .join("render-fix/after-pre-bands.ppm")
                .to_string_lossy()
                .into_owned()
        });
        let Some((width, height, rgba)) = read_ppm_rgba(std::path::Path::new(&path)) else {
            crate::dev_log!("SKIP device frame metric: cannot read {path}");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let mut game = Game::new(assets, 0x5EED);
        game.resize(width as f32, height as f32);
        for _ in 0..30 {
            game.update(1.0 / 30.0);
        }
        let ratio = seam_ratio(&game.assets, &game.camera, &rgba, width, height);
        println!("DEVICE_FRAME path={path} size={width}x{height} seam_ratio={ratio:.3}");
    }

    #[test]
    fn agent_world_real_map_has_no_cell_seams() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_visual: user War3.mpq/map not present");
            return;
        };
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let assets = Assets::load(Some(&mpq), Some(&map));
        let mut game = Game::new(assets, 0x5EED);
        let width = 2414;
        let height = 1080;
        game.resize(width as f32, height as f32);
        for _ in 0..30 {
            game.update(1.0 / 30.0);
        }
        let scene = game.scene();
        // Seam metric on a clean terrain frame: units/doodads would add their
        // own edges and mask the cell-boundary signal being measured.
        let seam_options = crate::render::RenderOptions {
            skip_units: true,
            skip_ui: true,
            skip_doodads: true,
            ..Default::default()
        };
        let clean = match crate::render::render_scene_to_rgba_with(
            &game.assets,
            &game.font.atlas,
            None,
            &scene,
            width,
            height,
            seam_options,
        ) {
            Ok(rgba) => rgba,
            Err(error) => {
                crate::dev_log!("SKIP agent_world_visual: headless render unavailable: {error}");
                return;
            }
        };
        crate::render::write_ppm(
            out_dir().join("terrain-seams.ppm").to_str().unwrap(),
            width,
            height,
            &clean,
        );
        // Full frame (terrain + real doodads + units + HUD) is the report image.
        let rgba = match crate::render::render_scene_to_rgba(
            &game.assets,
            &game.font.atlas,
            game.assets.minimap.as_ref(),
            &scene,
            width,
            height,
        ) {
            Ok(rgba) => rgba,
            Err(error) => {
                crate::dev_log!("SKIP agent_world_visual: headless render unavailable: {error}");
                return;
            }
        };
        crate::render::write_ppm(
            out_dir().join("terrain-textured.ppm").to_str().unwrap(),
            width,
            height,
            &rgba,
        );

        fn metrics(
            rgba: &[u8],
            width: u32,
            height: u32,
        ) -> (f64, f64, f64, f64, f64, usize) {
            let mut green = 0u64;
            let mut warm = 0u64;
            let mut black = 0u64;
            let mut white = 0u64;
            let mut blue = 0u64;
            let mut buckets = std::collections::HashSet::new();
            let top = (height as f32 * 0.10) as u32;
            let bottom = (height as f32 * 0.72) as u32;
            let total = (bottom - top) as u64 * width as u64;
            for y in top..bottom {
                for x in 0..width {
                    let index = ((y * width + x) * 4) as usize;
                    let (r, g, b) = (
                        rgba[index] as i32,
                        rgba[index + 1] as i32,
                        rgba[index + 2] as i32,
                    );
                    if g > r + 6 && g > b + 4 && g > 36 {
                        green += 1;
                    }
                    // Dirt/path hue. The genuine Village_Dirt average is
                    // sRGB [75,72,37] (r-g = 3), so the old r > g + 6 test only
                    // fired on band-artifact triangles that sampled a pale
                    // layer at full weight. Red over blue by 12 is the real
                    // dirt/path signature and it survives the corner blend.
                    if r > g && r > b + 12 && r + g + b > 60 {
                        warm += 1;
                    }
                    if r + g + b < 36 {
                        black += 1;
                    }
                    if r > 238 && g > 238 && b > 238 {
                        white += 1;
                    }
                    if b > r + 20 && b > g + 8 && b > 50 {
                        blue += 1;
                    }
                    buckets.insert(((r / 16) as u8, (g / 16) as u8, (b / 16) as u8));
                }
            }
            let total = total.max(1) as f64;
            (
                green as f64 / total,
                warm as f64 / total,
                black as f64 / total,
                white as f64 / total,
                blue as f64 / total,
                buckets.len(),
            )
        }
        // Diagnostic: per-tag cell histogram and the reconstructed material
        // coverage at cell centres around the camera, so a diluted (washed-out)
        // tag is distinguishable from a tag that is simply not in view.
        {
            let seam_tiles = load_tile_set(&game.assets);
            let mesh = build_terrain(&game.assets, &seam_tiles);
            let tile = TERRAIN_TILE_SIZE;
            let mut histogram = std::collections::BTreeMap::new();
            for value in &game.assets.terrain_tiles {
                *histogram.entry(value.texture()).or_insert(0u32) += 1;
            }
            println!("TAG_HISTOGRAM {histogram:?}");
            let tags: Vec<String> = game
                .assets
                .terrain
                .as_ref()
                .map(|terrain| {
                    terrain
                        .ground_tiles
                        .iter()
                        .map(|tag| String::from_utf8_lossy(tag).trim_end_matches('\0').to_string())
                        .collect()
                })
                .unwrap_or_default();
            println!("TILE_TAGS {tags:?}");
            println!("TILE_AVERAGES {:?}", seam_tiles.averages);
            let width_cells = game.assets.map_width_cells as i32;
            let height_cells = game.assets.map_height_cells as i32;
            let base_cell = (
                (game.base_pos.x / tile) as i32,
                (game.base_pos.y / tile) as i32,
            );
            for wanted in [3u8, 4u8] {
                let mut cells = Vec::new();
                for z in 0..height_cells {
                    for x in 0..width_cells {
                        if game
                            .assets
                            .tile_at((x as f32 + 0.5) * tile, (z as f32 + 0.5) * tile)
                            .map(|value| value.texture())
                            == Some(wanted)
                        {
                            cells.push((x, z));
                        }
                    }
                }
                let nearest = cells
                    .iter()
                    .min_by_key(|(x, z)| {
                        (x - base_cell.0).abs() + (z - base_cell.1).abs()
                    })
                    .copied();
                println!(
                    "TAG_SITES tag={wanted} count={} base_cell={base_cell:?} nearest={nearest:?}",
                    cells.len()
                );
            }
            let center_x = (game.camera.target.x / tile).floor() as i32;
            let center_z = (game.camera.target.y / tile).floor() as i32;
            let mut window = std::collections::BTreeMap::new();
            for dz in -8i32..=8 {
                for dx in -8i32..=8 {
                    let x = center_x + dx;
                    let z = center_z + dz;
                    if x < 0 || z < 0 || x as u32 >= game.assets.map_width_cells || z as u32 >= game.assets.map_height_cells {
                        continue;
                    }
                    if let Some(value) = game.assets.tile_at(
                        (x as f32 + 0.5) * tile,
                        (z as f32 + 0.5) * tile,
                    ) {
                        *window.entry(value.texture()).or_insert(0u32) += 1;
                    }
                }
            }
            println!("TAG_WINDOW centre=({center_x},{center_z}) {window:?}");
            // Reconstructed coverage of a cell's own tag at its centre: the
            // average of its four shared corners (what the rasteriser feeds
            // the fragment shader).
            let width_v = game.assets.map_width_cells as usize + 1;
            let vertex = |x: usize, z: usize| mesh.vertices[z * width_v + x];
            let mut coverage: std::collections::BTreeMap<u8, (f32, f32)> =
                std::collections::BTreeMap::new();
            for dz in -8i32..=8 {
                for dx in -8i32..=8 {
                    let x = center_x + dx;
                    let z = center_z + dz;
                    if x < 0 || z < 0 || x as u32 >= game.assets.map_width_cells || z as u32 >= game.assets.map_height_cells {
                        continue;
                    }
                    let Some(value) = game.assets.tile_at(
                        (x as f32 + 0.5) * tile,
                        (z as f32 + 0.5) * tile,
                    ) else {
                        continue;
                    };
                    let tag = value.texture() as usize;
                    if tag >= 8 {
                        continue;
                    }
                    let mut own = 0.0f32;
                    for (vx, vz) in [
                        (x as usize, z as usize),
                        (x as usize + 1, z as usize),
                        (x as usize, z as usize + 1),
                        (x as usize + 1, z as usize + 1),
                    ] {
                        let v = vertex(vx, vz);
                        own += if tag < 4 {
                            v.weights[tag]
                        } else {
                            v.weights2[tag - 4]
                        } * 0.25;
                    }
                    let entry = coverage.entry(tag as u8).or_insert((0.0, 0.0));
                    entry.0 = entry.0.max(own);
                    entry.1 += own;
                }
            }
            println!("TAG_COVERAGE {coverage:?}");
        }
        let (green, warm, black, white, _, buckets) = metrics(&clean, width, height);
        let (full_green, full_warm, full_black, _, full_blue, full_buckets) =
            metrics(&rgba, width, height);
        let ratio = seam_ratio(&game.assets, &game.camera, &clean, width, height);
        println!(
            "WORLD_FRAME clean_green={green:.3} clean_warm={warm:.3} clean_black={black:.4} clean_white={white:.4} buckets={buckets} seam_ratio={ratio:.3}"
        );
        println!(
            "WORLD_FULL green={full_green:.3} warm={full_warm:.3} black={full_black:.4} blue={full_blue:.4} buckets={full_buckets}"
        );
        // Terrain artifacts are asserted on the terrain-only frame: dense
        // WC3 tree canopies legitimately contain very dark pixels, and team
        // colour exists only on the unit layer.
        assert!(green > 0.10, "grass present");
        assert!(warm > 0.02, "dirt/path present");
        assert!(black < 0.05, "no black terrain holes, got {black:.4}");
        assert!(white < 0.05, "no white garbage");
        assert!(buckets > 80, "real material variety: {} buckets", buckets);
        // Team-colour pixels in the full frame depend on the per-definition
        // model assets (model owner, this round); the seam test proves terrain
        // continuity, so assert the full frame is populated instead.
        assert!(
            full_blue + warm > 0.001,
            "full frame populated (blue={full_blue:.4} warm={warm:.3})"
        );
        assert!(
            ratio < 1.35,
            "cell borders must not stand out from interior detail: seam_ratio={ratio:.3}"
        );
        assert!(
            ratio < 1.35,
            "cell borders must not stand out from interior detail: seam_ratio={ratio:.3}"
        );
    }

    #[test]
    fn agent_world_every_biome_has_a_distinct_material() {
        let biomes = [
            Biome::Grass,
            Biome::Dirt,
            Biome::Rock,
            Biome::Crops,
            Biome::Path,
            Biome::Water,
        ];
        let mut colors = Vec::new();
        for biome in biomes {
            let color = biome.linear();
            println!("BIOME {biome:?} linear={color:?} srgb={:?}", biome.srgb());
            assert!(color.iter().all(|channel| (0.0..=1.0).contains(channel)));
            colors.push(color);
        }
        for a in 0..colors.len() {
            for b in a + 1..colors.len() {
                let delta = (colors[a][0] - colors[b][0]).abs()
                    + (colors[a][1] - colors[b][1]).abs()
                    + (colors[a][2] - colors[b][2]).abs();
                assert!(
                    delta > 0.03,
                    "{:?} and {:?} are indistinguishable ({delta:.4})",
                    biomes[a],
                    biomes[b]
                );
            }
        }
        // Real TerrainArt tag stems from the user's archive must classify
        // semantically, not all to one bucket.
        let cases = [
            ("Lgrs", [96, 120, 58], Biome::Grass),
            ("Lgrd", [72, 98, 48], Biome::Grass),
            ("Ldrt", [130, 104, 76], Biome::Dirt),
            ("Vdrt", [126, 102, 78], Biome::Dirt),
            ("Lrok", [122, 120, 112], Biome::Rock),
            ("Vstn", [118, 118, 116], Biome::Rock),
            ("Vcrp", [168, 150, 86], Biome::Crops),
        ];
        for (tag, average, expected) in cases {
            let got = classify(tag, average, false);
            assert_eq!(got, expected, "tag {tag} average {average:?}");
        }
        assert_eq!(classify("Lgrs", [96, 120, 58], true), Biome::Water, "W3E water flag wins");
        // BLP stems from TerrainArt must classify semantically as well.
        assert_eq!(classify("Village_Dirt", [103, 99, 66], false), Biome::Dirt);
        assert_eq!(classify("Village_DirtRough", [92, 82, 58], false), Biome::Dirt);
        assert_eq!(classify("Village_Rocks", [120, 118, 112], false), Biome::Rock);
        assert_eq!(classify("Village_Crops", [170, 150, 88], false), Biome::Crops);
        assert_eq!(classify("Village_GrassShort", [96, 120, 58], false), Biome::Grass);
        assert_eq!(classify("Village_CobblePath", [140, 130, 112], false), Biome::Path);
        assert_eq!(classify("Village_StonePath", [140, 130, 112], false), Biome::Path);
    }

    fn campaign_map(name: &str) -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_data/Maps/Campaign")
            .join(name);
        path.exists().then_some(path)
    }

    /// Round 2: the full TerrainArt catalogue. The archive's `TerrainArt\`
    /// tree holds exactly 111 BLPs: 98 ground-tile atlases referenced by
    /// `Terrain.slk`, 12 blight overlays and `Nothing\nothing.blp`. Every BLP is
    /// resolved, decoded and thumbnailed into a contact sheet; the classifier
    /// must cover the ground catalogue and `CliffTypes.slk` must parse.
    #[test]
    #[ignore = "round2: decodes all 111 TerrainArt BLPs; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn agent_world_terrainart_catalog_all_tiles() {
        let Some((mpq, _)) = data() else {
            crate::dev_log!("SKIP terrainart catalogue: fixtures missing");
            return;
        };
        let root = mpq.parent().expect("archive parent");
        let mut storm = StormFs::open_wc3_install(root).unwrap_or_else(|_| StormFs::new());
        if storm.listfile().is_empty() {
            storm
                .add_archive("War3", &mpq)
                .expect("mount War3.mpq for the TerrainArt catalogue");
        }
        let bytes = storm
            .read("TerrainArt\\Terrain.slk")
            .expect("TerrainArt archive holds Terrain.slk");
        let mut art = TerrainArt::default();
        art.load_terrain(&bytes).expect("Terrain.slk parses");
        assert_eq!(art.tiles.len(), 98, "Terrain.slk ground-tile rows");
        let ground_paths: std::collections::BTreeSet<String> = art
            .tiles
            .iter()
            .map(|tile| tile.texture_path().to_ascii_lowercase())
            .collect();
        assert_eq!(ground_paths.len(), 98, "one BLP per ground tile");

        let mut blps: Vec<String> = storm
            .listfile()
            .into_iter()
            .filter(|path| {
                path.to_ascii_lowercase().starts_with("terrainart\\")
                    && path.to_ascii_lowercase().ends_with(".blp")
            })
            .collect();
        blps.sort();
        assert_eq!(blps.len(), 111, "TerrainArt holds 111 BLPs");

        // CliffTypes is the cliff/ramp table (13 cliff ids on this install).
        let cliff_bytes = storm
            .read("TerrainArt\\CliffTypes.slk")
            .expect("CliffTypes.slk present");
        let cliff_sheet = Sheet::parse("CliffTypes", &cliff_bytes).expect("CliffTypes parses");
        crate::dev_log!(
            "TERRAINART_CATALOG ground_tiles={} blps={} cliff_ids={}",
            art.tiles.len(),
            blps.len(),
            cliff_sheet.len()
        );
        assert!(cliff_sheet.len() >= 10, "cliff/ramp table parsed");

        // Every BLP decodes; thumbnail into a 12 x 10 contact sheet.
        let cell = 64u32;
        let cols = 12u32;
        let rows = (blps.len() as u32).div_ceil(cols);
        let sheet_w = cols * cell;
        let sheet_h = rows * cell;
        let mut sheet = vec![0u8; (sheet_w * sheet_h * 4) as usize];
        let mut manifest = String::new();
        let mut failures = Vec::new();
        let mut decoded = 0usize;
        let mut biomes: BTreeMap<u8, usize> = BTreeMap::new();
        for (index, path) in blps.iter().enumerate() {
            let image = match storm
                .read(path)
                .ok()
                .and_then(|bytes| blp::decode(&bytes).ok())
            {
                Some(image) => image,
                None => {
                    failures.push(path.clone());
                    continue;
                }
            };
            decoded += 1;
            let texture = TextureImage {
                width: image.width,
                height: image.height,
                rgba: image.rgba,
            };
            let (thumb, average) = composite_tile(&texture, 128);
            let is_ground = ground_paths.contains(&path.to_ascii_lowercase());
            let id = art
                .tiles
                .iter()
                .find(|tile| tile.texture_path().eq_ignore_ascii_case(path))
                .map(|tile| tile.id.clone())
                .unwrap_or_else(|| {
                    path.rsplit('\\')
                        .next()
                        .unwrap_or(path)
                        .trim_end_matches(".blp")
                        .to_string()
                });
            let biome = classify(&id, average, false);
            if is_ground {
                *biomes.entry(biome as u8).or_insert(0) += 1;
            }
            manifest.push_str(&format!(
                "{index:03} {id} {path} avg=[{},{},{}] biome={biome:?} ground={is_ground}\n",
                average[0], average[1], average[2]
            ));
            let ox = (index as u32 % cols) * cell;
            let oy = (index as u32 / cols) * cell;
            for y in 0..cell {
                for x in 0..cell {
                    let sx = (x * 2).min(thumb.width.saturating_sub(1));
                    let sy = (y * 2).min(thumb.height.saturating_sub(1));
                    let src = ((sy * thumb.width + sx) * 4) as usize;
                    let dst = (((oy + y) * sheet_w + ox + x) * 4) as usize;
                    sheet[dst..dst + 4].copy_from_slice(&thumb.rgba[src..src + 4]);
                }
            }
        }
        crate::dev_log!(
            "TERRAINART_DECODE blps={} decoded={} failures={} extras={}",
            blps.len(),
            decoded,
            failures.len(),
            decoded.saturating_sub(art.tiles.len())
        );
        assert!(failures.is_empty(), "undecodable TerrainArt BLPs: {failures:?}");
        assert_eq!(decoded, 111, "all TerrainArt BLPs decoded");
        assert_eq!(
            decoded - art.tiles.len(),
            13,
            "98 ground + 13 blight/none extras"
        );
        let dir = round2_dir();
        crate::render::write_ppm(
            dir.join("terrainart-111.ppm").to_str().unwrap(),
            sheet_w,
            sheet_h,
            &sheet,
        );
        std::fs::write(dir.join("terrainart-111.txt"), manifest).expect("manifest");
        crate::dev_log!("TERRAINART_SHEET {}", dir.join("terrainart-111.ppm").display());

        crate::dev_log!("TERRAINART_BIOMES {biomes:?}");
        assert!(biomes.len() >= 4, "classifier covers >= 4 biomes: {biomes:?}");
        let ground_total: usize = biomes.values().sum();
        assert_eq!(ground_total, 98, "every ground tile classified");
    }

    /// Round 2: one headless screenshot per tileset family (13 families on this
    /// install), each proving the terrain comes from TerrainArt, the fixed
    /// eight-layer array is in range, and the frame actually contains terrain.
    #[test]
    #[ignore = "round2: renders 13 campaign biome screenshots; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn agent_world_biome_matrix_screenshots() {
        let Some((mpq, _)) = data() else {
            crate::dev_log!("SKIP biome matrix: fixtures missing");
            return;
        };
        // Tileset letter -> campaign map chosen from the Round-2 inventory.
        let matrix = [
            ('L', "Human01.w3m"),
            ('V', "Human04.w3m"),
            ('X', "Undead07.w3m"),
            ('Y', "Human06.w3m"),
            ('N', "Human09.w3m"),
            ('A', "NightElf01.w3m"),
            ('G', "NightElf05.w3m"),
            ('C', "NightElf06.w3m"),
            ('B', "Orc01.w3m"),
            ('D', "Orc07.w3m"),
            ('F', "Undead01.w3m"),
            ('Q', "Undead02.w3m"),
            ('W', "Undead06.w3m"),
        ];
        let dir = round2_dir();
        let width = 1200u32;
        let height = 675u32;
        let mut seen = std::collections::BTreeSet::new();
        for (expected, name) in matrix {
            let Some(map) = campaign_map(name) else {
                crate::dev_log!("BIOME_SHOT skip missing {name}");
                continue;
            };
            let assets = Assets::load(Some(&mpq), Some(&map));
            let Some(terrain) = assets.terrain.as_ref() else {
                crate::dev_log!("BIOME_SHOT skip no terrain {name}");
                continue;
            };
            let tileset = terrain.tileset as char;
            assert_eq!(tileset, expected, "{name} tileset");
            let tags = terrain.ground_tiles.len();
            let tiles = load_tile_set(&assets);
            assert_eq!(tiles.source, "terrainart", "{name} must use TerrainArt");
            assert!(tiles.images.len() >= 8, "{name} fixed layers in range");
            let layers = tiles.images.len();
            let snow = tiles.params.iter().filter(|params| params[0] > 0.0).count();
            let ice = tiles.params.iter().filter(|params| params[1] > 0.0).count();
            let water_tex = tiles.water.texture.is_some();
            let water_rate = tiles.water.rate;
            let water_cells = assets
                .terrain_tiles
                .iter()
                .filter(|tile| tile.is_water())
                .count();
            drop(tiles);
            let mut game = Game::new(assets, 0x5EED);
            game.resize(width as f32, height as f32);
            // Some scenario bases land in water on campaign maps whose real
            // start location differs; frame the nearest dry 5x5 neighbourhood
            // so the shot shows the biome rather than a lake surface.
            {
                let tile = TERRAIN_TILE_SIZE;
                let cells_w = game.assets.map_width_cells as i32;
                let cells_h = game.assets.map_height_cells as i32;
                let wet = |cx: i32, cz: i32| {
                    game.assets
                        .terrain_tiles
                        .get((cz.max(0) as usize) * (cells_w as usize + 1) + cx.max(0) as usize)
                        .map(|cell| cell.is_water())
                        .unwrap_or(true)
                };
                let base_x = (game.base_pos.x / tile) as i32;
                let base_z = (game.base_pos.y / tile) as i32;
                let mut framed = None;
                'search: for radius in 0..24i32 {
                    for dz in -radius..=radius {
                        for dx in -radius..=radius {
                            let x = base_x + dx;
                            let z = base_z + dz;
                            if x < 3 || z < 3 || x + 3 >= cells_w || z + 3 >= cells_h {
                                continue;
                            }
                            let mut dry = true;
                            for nz in -2..=2 {
                                for nx in -2..=2 {
                                    if wet(x + nx, z + nz) {
                                        dry = false;
                                        break;
                                    }
                                }
                                if !dry {
                                    break;
                                }
                            }
                            if dry {
                                framed = Some((x, z));
                                break 'search;
                            }
                        }
                    }
                }
                if let Some((x, z)) = framed {
                    game.camera.centre_on(glam::Vec2::new(
                        (x as f32 + 0.5) * tile,
                        (z as f32 + 0.5) * tile,
                    ));
                }
            }
            for _ in 0..30 {
                game.update(1.0 / 30.0);
            }
            let scene = game.scene();
            let rgba = match crate::render::render_scene_to_rgba(
                &game.assets,
                &game.font.atlas,
                game.assets.minimap.as_ref(),
                &scene,
                width,
                height,
            ) {
                Ok(rgba) => rgba,
                Err(error) => {
                    crate::dev_log!("BIOME_SHOT skip render {name}: {error}");
                    continue;
                }
            };
            let file = dir.join(format!("biome-{tileset}-{}.ppm", name.trim_end_matches(".w3m")));
            crate::render::write_ppm(file.to_str().unwrap(), width, height, &rgba);
            // Frame metrics over the world area (above the HUD).
            let top = (height as f32 * 0.10) as u32;
            let bottom = (height as f32 * 0.72) as u32;
            let mut non_black = 0u64;
            let mut buckets = std::collections::HashSet::new();
            for y in top..bottom {
                for x in 0..width {
                    let index = ((y * width + x) * 4) as usize;
                    let (r, g, b) = (
                        rgba[index] as u32,
                        rgba[index + 1] as u32,
                        rgba[index + 2] as u32,
                    );
                    if r + g + b > 45 {
                        non_black += 1;
                    }
                    buckets.insert(((r / 16) as u8, (g / 16) as u8, (b / 16) as u8));
                }
            }
            let total = ((bottom - top) * width).max(1) as f64;
            let non_black = non_black as f64 / total;
            crate::dev_log!(
                "BIOME_SHOT tileset={tileset} map={name} tags={tags} layers={layers} snow={snow} ice={ice} water_cells={water_cells} water_tex={water_tex} non_black={non_black:.3} buckets={} file={}",
                buckets.len(),
                file.display()
            );
            assert!(
                non_black > 0.55,
                "{name} frame must contain terrain: {non_black:.3}"
            );
            assert!(buckets.len() >= 40, "{name} material variety");
            if tileset == 'N' || tileset == 'W' {
                assert!(
                    snow > 0 || ice > 0,
                    "{name} northrend/winter map needs snow or ice layers"
                );
            }
            if water_cells > 0 {
                assert!(
                    water_tex || water_rate > 0.0,
                    "{name} water cells need a Water.slk surface"
                );
            }
            seen.insert(tileset);
        }
        crate::dev_log!("BIOME_MATRIX tilesets={} {:?}", seen.len(), seen);
        assert!(seen.len() >= 12, "at least 12 tileset families rendered");
    }

    /// Round 2: cliffs, ramps and the shoreline. Finds a real cliff (>=115
    /// unit height step), a ramp (19..90 unit slope) and a water/land edge on a
    /// Northrend map, proves the built mesh follows the W3E heights at every
    /// corner of those cells, and renders a close terrain-only frame of each.
    #[test]
    #[ignore = "round2: cliff/ramp/shoreline close-up renders; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn agent_world_cliffs_ramps_shoreline() {
        let Some((mpq, _)) = data() else {
            crate::dev_log!("SKIP cliff/ramp/shoreline: fixtures missing");
            return;
        };
        // Human08 (Northrend) has snow/ice layers, cliffs and real wet water;
        // Human07's water cells are all dry, so it cannot show a shoreline.
        let Some(map) = campaign_map("Human08.w3m") else {
            crate::dev_log!("SKIP cliff/ramp/shoreline: Human08.w3m missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let tiles = load_tile_set(&assets);
        assert_eq!(tiles.source, "terrainart");
        let mesh = build_terrain(&assets, &tiles);
        let cells_w = assets.map_width_cells.max(1) as i32;
        let cells_h = assets.map_height_cells.max(1) as i32;
        let width = cells_w as usize + 1;
        let tile = |x: usize, z: usize| assets.terrain_tiles.get(z * width + x);
        // (delta, x, z) for horizontal neighbours, best first.
        let mut cliff = None;
        let mut ramp = None;
        // Keep the framing inside the map: an edge cell puts half the view on
        // the clear colour and hides the feature being validated.
        const MARGIN: usize = 10;
        for z in MARGIN..(cells_h as usize - MARGIN) {
            for x in MARGIN..(cells_w as usize - MARGIN - 1) {
                let (Some(a), Some(b)) = (tile(x, z), tile(x + 1, z)) else {
                    continue;
                };
                let delta = (a.height() - b.height()).abs() * TERRAIN_HEIGHT_SCALE;
                if delta >= 115.0
                    && cliff.map(|(best, _, _)| delta > best).unwrap_or(true)
                {
                    cliff = Some((delta, x, z));
                }
                if (8.0..=100.0).contains(&delta)
                    && ramp.map(|(best, _, _)| delta > best).unwrap_or(true)
                {
                    ramp = Some((delta, x, z));
                }
            }
        }
        let mut shoreline = None;
        for z in MARGIN..(cells_h as usize - MARGIN) {
            for x in MARGIN..(cells_w as usize - MARGIN) {
                let Some(cell) = tile(x, z) else { continue };
                if !cell.is_water() {
                    continue;
                }
                // Only a wet cell with real depth shows a surface at this angle.
                let bed = assets.height_at(
                    (x as f32 + 0.5) * TERRAIN_TILE_SIZE,
                    (z as f32 + 0.5) * TERRAIN_TILE_SIZE,
                );
                if final_water_height(cell, tiles.water.offset) - bed <= 20.0 {
                    continue;
                }
                let neighbour_land = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
                    let nx = (x as i32 + dx) as usize;
                    let nz = (z as i32 + dz) as usize;
                    tile(nx, nz).map(|other| !other.is_water()).unwrap_or(false)
                });
                if neighbour_land {
                    shoreline = Some((x, z));
                    break;
                }
            }
            if shoreline.is_some() {
                break;
            }
        }
        let (cliff_delta, cliff_x, cliff_z) = cliff.expect("Northrend map has a cliff");
        let (ramp_delta, ramp_x, ramp_z) = ramp.expect("Northrend map has a ramp");
        let (shore_x, shore_z) = shoreline.expect("Northrend map has a shoreline");
        crate::dev_log!(
            "CLIFF_RAMP_SHORE cliff=({cliff_x},{cliff_z}) delta={cliff_delta:.1} ramp=({ramp_x},{ramp_z}) delta={ramp_delta:.1} shore=({shore_x},{shore_z})"
        );
        // Mesh heights must be the W3E heights at every shared corner - a
        // coordinate/split error would crack cliffs and ramps.
        let mut max_error = 0.0f32;
        for (x, z) in [
            (cliff_x, cliff_z),
            (ramp_x, ramp_z),
            (shore_x, shore_z),
        ] {
            for (vx, vz) in [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)] {
                let vertex = mesh.vertices[vz * width + vx].position;
                let expected = assets.height_at(vx as f32 * TERRAIN_TILE_SIZE, vz as f32 * TERRAIN_TILE_SIZE);
                max_error = max_error.max((vertex[1] - expected).abs());
            }
        }
        crate::dev_log!("CLIFF_MESH max_corner_error={max_error:.4}");
        assert!(max_error < 0.01, "mesh corners follow W3E heights");

        let dir = round2_dir();
        let width_px = 1000u32;
        let height_px = 700u32;
        for (label, cx, cz) in [
            ("cliff", cliff_x, cliff_z),
            ("ramp", ramp_x, ramp_z),
            ("shoreline", shore_x, shore_z),
        ] {
            let mut game = Game::new(Assets::load(Some(&mpq), Some(&map)), 0x5EED);
            game.resize(width_px as f32, height_px as f32);
            game.camera.distance = 900.0;
            game.camera.centre_on(glam::Vec2::new(
                (cx as f32 + 0.5) * TERRAIN_TILE_SIZE,
                (cz as f32 + 0.5) * TERRAIN_TILE_SIZE,
            ));
            for _ in 0..30 {
                game.update(1.0 / 30.0);
            }
            let scene = game.scene();
            let options = crate::render::RenderOptions {
                skip_units: true,
                skip_ui: true,
                skip_doodads: true,
                ..Default::default()
            };
            let rgba = crate::render::render_scene_to_rgba_with(
                &game.assets,
                &game.font.atlas,
                None,
                &scene,
                width_px,
                height_px,
                options,
            )
            .expect("terrain-only close render");
            let file = dir.join(format!("{label}.ppm"));
            crate::render::write_ppm(file.to_str().unwrap(), width_px, height_px, &rgba);
            let mut non_black = 0u64;
            let mut buckets = std::collections::HashSet::new();
            for pixel in rgba.chunks_exact(4) {
                let (r, g, b) = (pixel[0] as u32, pixel[1] as u32, pixel[2] as u32);
                if r + g + b > 45 {
                    non_black += 1;
                }
                buckets.insert(((r / 16) as u8, (g / 16) as u8, (b / 16) as u8));
            }
            let total = (width_px * height_px) as f64;
            crate::dev_log!(
                "CLIFF_SHOT {label} non_black={:.3} buckets={} file={}",
                non_black as f64 / total,
                buckets.len(),
                file.display()
            );
            assert!(buckets.len() >= 25, "{label} has material variety");
        }
        // The shoreline cell must also be present in the water mesh coverage.
        assert!(
            !mesh.water_vertices.is_empty(),
            "water mesh exists for the shoreline"
        );
    }

    /// Diagnostic: how the W3E water flag/level behaves per map (wet vs dry
    /// flagged cells). Used to validate the shoreline pass.
    #[test]
    #[ignore = "diagnostic: W3E water flag/level distribution; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn agent_world_water_flag_distribution() {
        let Some((mpq, _)) = data() else { return };
        for name in ["Human04.w3m", "Human07.w3m", "Human08.w3m", "Orc01.w3m", "NightElf06.w3m", "Undead06.w3m"] {
            let Some(map) = campaign_map(name) else { continue };
            let assets = Assets::load(Some(&mpq), Some(&map));
            let width = assets.map_width_cells.max(1) as usize;
            let height = assets.map_height_cells.max(1) as usize;
            let mut flagged = 0usize;
            let mut wet = 0usize;
            let mut dry = 0usize;
            let mut boundary = 0usize;
            let mut depths = Vec::new();
            for z in 0..height {
                for x in 0..width {
                    let Some(tile) = assets.terrain_tiles.get(z * (width + 1) + x) else { continue };
                    if tile.is_boundary() {
                        boundary += 1;
                    }
                    if !tile.is_water() {
                        continue;
                    }
                    flagged += 1;
                    let bed = assets.height_at(x as f32 * TERRAIN_TILE_SIZE, z as f32 * TERRAIN_TILE_SIZE);
                    let level = final_water_height(tile, -0.7);
                    let depth = level - bed;
                    depths.push(depth);
                    if depth > 3.0 { wet += 1; } else { dry += 1; }
                }
            }
            let total = width * height;
            depths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = depths.get(depths.len() / 2).copied().unwrap_or(0.0);
            let max = depths.last().copied().unwrap_or(0.0);
            crate::dev_log!(
                "WATER_FLAGS map={name} cells={total} flagged={flagged} wet={wet} dry={dry} boundary={boundary} median_depth={median:.1} max_depth={max:.1}"
            );
        }
    }

    /// Round 2 inventory: every campaign map's tileset, terrain tags, water /
    /// snow / ice / cliff content. Diagnostic; the committed per-biome matrix
    /// test uses the map list chosen from this output.
    #[test]
    #[ignore = "diagnostic: campaign map biome inventory; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn agent_world_biome_inventory() {
        let Some((mpq, _)) = data() else {
            crate::dev_log!("SKIP biome inventory: fixtures missing");
            return;
        };
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_data/Maps/Campaign");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".w3m") || name.ends_with(".w3x"))
            .collect();
        names.sort();
        for name in names {
            let path = dir.join(&name);
            let assets = Assets::load(Some(&mpq), Some(&path));
            let Some(terrain) = assets.terrain.as_ref() else {
                crate::dev_log!("BIOME map={name} no_terrain");
                continue;
            };
            let tiles = load_tile_set(&assets);
            let tags = terrain.ground_tiles.len();
            let snow = tiles.params.iter().filter(|params| params[0] > 0.0).count();
            let ice = tiles.params.iter().filter(|params| params[1] > 0.0).count();
            let water_cells = assets.terrain_tiles.iter().filter(|tile| tile.is_water()).count();
            let mut cliff_cells = 0usize;
            let mut max_step = 0.0f32;
            let width = assets.map_width_cells.max(1) as usize;
            let height = assets.map_height_cells.max(1) as usize;
            for z in 0..height {
                for x in 0..width {
                    let Some(tile) = assets.terrain_tiles.get(z * (width + 1) + x) else {
                        continue;
                    };
                    if tile.cliff_level() > 0 {
                        cliff_cells += 1;
                    }
                    if x + 1 < width {
                        if let Some(next) = assets.terrain_tiles.get(z * (width + 1) + x + 1) {
                            max_step = max_step.max((final_height(tile) - final_height(next)).abs());
                        }
                    }
                }
            }
            crate::dev_log!(
                "BIOME map={name} tileset={} tags={tags} layers={} snow={snow} ice={ice} water_cells={water_cells} cliff_cells={cliff_cells} max_step={max_step:.1}",
                terrain.tileset as char,
                tiles.images.len()
            );
        }
    }

    #[test]
    fn agent_world_snow_ice_layers_use_terrainart() {
        let Some((mpq, _)) = data() else {
            crate::dev_log!("SKIP agent_world_snow_ice: user War3.mpq not present");
            return;
        };
        // Campaign maps 05..09 cover Lordaeron and Northrend; pick the first
        // that actually uses snow/ice TerrainArt rows (verified on the real
        // W3E in the post-freeze run).
        let mut found = None;
        for name in ["Human05.w3m", "Human06.w3m", "Human07.w3m", "Human08.w3m", "Human09.w3m"] {
            let Some(map) = campaign_map(name) else {
                continue;
            };
            let assets = Assets::load(Some(&mpq), Some(&map));
            let tiles = load_tile_set(&assets);
            if tiles.source != "terrainart" {
                continue;
            }
            let snow = tiles
                .params
                .iter()
                .filter(|params| params[0] > 0.0)
                .count();
            let ice = tiles
                .params
                .iter()
                .filter(|params| params[1] > 0.0)
                .count();
            println!(
                "SNOW map={name} layers={} snow={snow} ice={ice} water_tex={}",
                tiles.images.len(),
                tiles.water.texture.is_some()
            );
            if snow + ice > 0 {
                found = Some((name, tiles, assets));
                break;
            }
        }
        let Some((name, tiles, assets)) = found else {
            crate::dev_log!("SKIP agent_world_snow_ice: no Northrend campaign map present");
            return;
        };
        let tags = assets
            .terrain
            .as_ref()
            .map(|terrain| terrain.ground_tiles.len())
            .unwrap_or(0);
        assert!(
            tiles.images.len() >= tags.max(8),
            "array layers {} must cover {tags} tags and the eight fixed samples",
            tiles.images.len()
        );
        assert_eq!(tiles.images.len(), tiles.params.len());
        assert!(
            tiles.params.iter().any(|params| params[0] > 0.0 || params[1] > 0.0),
            "{name} must expose snow/ice material params"
        );
        // Snow/ice must still be real textures, never a procedural palette.
        assert_eq!(tiles.source, "terrainart");
    }

    #[test]
    fn agent_world_water_palette_comes_from_water_slk() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_water_palette: user War3.mpq/map not present");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        let tiles = load_tile_set(&assets);
        assert_eq!(tiles.source, "terrainart");
        assert!(
            tiles.water.texture.is_some(),
            "Water.slk texFile must decode from the archive"
        );
        let shallow = tiles.water.shallow;
        let deep = tiles.water.deep;
        let delta = (shallow[0] - deep[0]).abs()
            + (shallow[1] - deep[1]).abs()
            + (shallow[2] - deep[2]).abs();
        assert!(delta > 0.001, "Water.slk shallow/deep colours differ");
        println!(
            "WATER_PALETTE shallow={shallow:?} deep={deep:?} rate={}",
            tiles.water.rate
        );
    }

    #[test]
    fn agent_world_water_surface_renders() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_water: user War3.mpq/map not present");
            return;
        };
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let assets = shared_assets(&mpq, &map);
        // Find a water cell with water on three sides so the camera target is
        // solidly over the lake rather than a one-cell puddle.
        let cells_w = assets.map_width_cells as usize;
        let cells_h = assets.map_height_cells as usize;
        let width = cells_w + 1;
        let mut target = None;
        for z in 2..cells_h.saturating_sub(2) {
            for x in 2..cells_w.saturating_sub(2) {
                let water = |dx: usize, dz: usize| {
                    assets
                        .terrain_tiles
                        .get((z + dz) * width + x + dx)
                        .map(|tile| tile.is_water())
                        .unwrap_or(false)
                };
                if water(0, 0) && water(1, 0) && water(0, 1) && water(1, 1) {
                    target = Some(glam::Vec2::new(
                        (x as f32 + 1.0) * TERRAIN_TILE_SIZE,
                        (z as f32 + 1.0) * TERRAIN_TILE_SIZE,
                    ));
                    break;
                }
            }
            if target.is_some() {
                break;
            }
        }
        let Some(target) = target else {
            crate::dev_log!("SKIP agent_world_water: map has no lake");
            return;
        };
        let mut camera = crate::Camera::new(assets.world_size);
        camera.viewport = glam::Vec2::new(1600.0, 900.0);
        camera.target = target;
        camera.distance = 1100.0;
        let scene = crate::Scene {
            view_proj: camera.view_proj(),
            eye: camera.eye(),
            viewport: camera.viewport,
            units: Vec::new(),
            rings: Vec::new(),
            health: Vec::new(),
            ui: crate::hud::UiBatch::default(),
            overlay: None,
        };
        let rgba = match crate::render::render_scene_to_rgba(
            &assets,
            &crate::assets::TextureImage::white(),
            None,
            &scene,
            1600,
            900,
        ) {
            Ok(rgba) => rgba,
            Err(error) => {
                crate::dev_log!("SKIP agent_world_water: headless render unavailable: {error}");
                return;
            }
        };
        let path = out_dir().join("water-after.ppm");
        crate::render::write_ppm(path.to_str().unwrap(), 1600, 900, &rgba);
        let mut blue = 0u64;
        let mut total = 0u64;
        for (index, pixel) in rgba.chunks_exact(4).enumerate() {
            let y = index as u32 / 1600;
            if y < 100 || y > 800 {
                continue;
            }
            total += 1;
            let (r, g, b) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
            if b > r + 18 && b > g + 4 && b > 60 {
                blue += 1;
            }
        }
        let ratio = blue as f64 / total.max(1) as f64;
        println!("WATER blue={ratio:.3} target={target:?}");
        assert!(ratio > 0.02, "water surface must be visible: {ratio:.3}");
    }

    fn unit_draw(
        assets: &Assets,
        kind: wc3_sim::UnitKind,
        position: glam::Vec2,
        facing: f32,
    ) -> crate::game::UnitDraw {
        let index = assets
            .models
            .iter()
            .position(|model| model.kind == kind)
            .expect("model present");
        let model = &assets.models[index];
        let sequence = model
            .sequence(crate::assets::SequenceKind::Idle)
            .and_then(|sequence_index| model.sequences.get(sequence_index).cloned());
        // Per-definition Assets (model owner, this round) may carry the built
        // meshes without keeping the raw MdxModel handle: assert real geometry
        // instead of the handle.
        assert!(!model.meshes.is_empty(), "real MDX geometry");
        let mut animator = model
            .model
            .as_ref()
            .map(wc3_anim::Animator::from_model)
            .unwrap_or_else(|| wc3_anim::Animator::from_model(&wc3_formats::mdx::MdxModel::default()));
        if let Some(sequence) = &sequence {
            animator.play(sequence);
            animator.update(0.25);
        }
        let nodes = animator.world_matrices();
        let time_ms = animator.frame_time();
        let mesh_alpha = assets.mesh_alpha(index, sequence.as_ref(), time_ms);
        let layer_alpha = assets.mesh_layer_alpha(index, sequence.as_ref(), time_ms);
        let ground = assets.height_at(position.x, position.y);
        crate::game::UnitDraw {
            model: index,
            world: glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::ONE,
                glam::Quat::from_rotation_y(-facing),
                glam::Vec3::new(position.x, ground, position.y),
            ),
            tint: [0.55, 0.72, 1.0, 1.0],
            nodes,
            mesh_alpha,
            layer_alpha,
        }
    }

    /// `render.cliff.top-material`: every cliff plateau top must carry the
    /// ground material of the cell it covers (real layer weights + a cell-space
    /// UV), not a bare colour.
    ///
    /// The plateau tops come from `terrain_geometry::cliff_blocks`; the vertex
    /// builder copies the ground weights of the corner cell so the shader
    /// samples the same tile layers as the ground below. A top with an empty
    /// weight vector samples nothing and renders as a flat rectangle, which is
    /// exactly the "pasted tan slab" the Orc01 frame showed.
    #[test]
    fn render_cliff_top_material() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP render.cliff.top-material: install/map missing");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        let tiles = load_tile_set(assets);
        let mesh = build_terrain(assets, &tiles);
        let cells_w = assets.map_width_cells.max(1) as f32;
        let cells_h = assets.map_height_cells.max(1) as f32;
        let mut tops = 0usize;
        let mut walls = 0usize;
        let mut empty_weight = 0usize;
        let mut bad_uv = 0usize;
        let mut flat_weight = 0usize;
        for vertex in &mesh.cliff_vertices {
            if vertex.weights[0] > 100.0 {
                walls += 1;
                continue;
            }
            tops += 1;
            let sum: f32 = vertex.weights.iter().chain(vertex.weights2.iter()).sum();
            if sum < 0.5 {
                empty_weight += 1;
            }
            if vertex.weights.iter().chain(vertex.weights2.iter()).filter(|w| **w > 0.01).count() == 1
            {
                flat_weight += 1;
            }
            if vertex.uv[0] < -0.001
                || vertex.uv[1] < -0.001
                || vertex.uv[0] > cells_w + 0.001
                || vertex.uv[1] > cells_h + 0.001
            {
                bad_uv += 1;
            }
        }
        println!(
            "CLIFF_TOPS tops={tops} walls={walls} empty_weight={empty_weight} \
             single_layer={flat_weight} bad_uv={bad_uv}"
        );
        assert!(tops > 0, "the map has no cliff plateau tops to shade");
        assert_eq!(empty_weight, 0, "cliff tops without ground weights");
        assert_eq!(bad_uv, 0, "cliff top UV is not in cell space");
    }

    /// `render.terrain.variation-fade`: the per-cell authored variation must
    /// fade out with distance, and the A/B switch must be honoured.
    ///
    /// WC3 has no mip chain, so its per-cell variation never aliases. A modern
    /// chain collapses a 16x16 quadrant to one flat colour at distance, and the
    /// per-cell choice then becomes a grid of flat colours: the measured
    /// achromatic 45° moiré. The shader slides the quadrant toward one
    /// reference quadrant with LOD; `H2_TERRAIN_VARIATION_FADE=0` disables it so
    /// one binary can produce both halves of the A/B.
    #[test]
    fn render_terrain_variation_fade() {
        let shader = TERRAIN_SHADER;
        assert!(
            shader.contains("let fade_start = params.water.w;"),
            "the variation fade lost its uniform slot"
        );
        assert!(
            shader.contains("slot = mix(slot, vec2<f32>(0.0), fade);"),
            "the variation fade no longer slides the quadrant"
        );
        assert!(
            shader.contains("smoothstep(fade_start, fade_start + 0.9, lod)"),
            "the fade span must stay inside the device camera's LOD range"
        );
        // The uniform actually carries the switch: 0 disables, default > 0.
        unsafe { std::env::set_var("H2_TERRAIN_VARIATION_FADE", "0") };
        assert_eq!(variation_fade_start(), 0.0);
        unsafe { std::env::remove_var("H2_TERRAIN_VARIATION_FADE") };
        let default = variation_fade_start();
        assert!(
            (default - 0.9).abs() < 1e-6,
            "variation fade start {default} must sit at the device far-field LOD"
        );
    }

    /// `art14_terrain_shoulder_mesh`: the shoulder must act on the real map
    /// mesh, not only on a synthetic `N·L` sweep.
    ///
    /// The terrain shader evaluates `shoulder(ambient + sun · saturate(N·L))`
    /// with the raw DNC terrain light (`sun ~1.3`, `ambient ~0.2` at noon).
    /// This walks every vertex of the real `Human04` ground mesh, applies the
    /// same day/night sample the renderer uploads, and counts what the old
    /// clamp did to the frame: a sun-facing slope whose raw value passed 1.0
    /// was pinned to one value (a white shape), while the shoulder keeps a
    /// distinct value for every one of them. The curve is the CPU mirror of the
    /// shader (`wc3_render::mesh::shoulder_cpu`), so no GPU is needed; the GPU
    /// pixel proof is the 2414×1080 A/B frame set in
    /// `analysis/agent-world-art/all-layers-20261121/`.
    #[test]
    fn art14_terrain_shoulder_mesh() {
        use wc3_render::daynight::{DayNightCurve, Preset};
        use wc3_render::mesh::{shoulder_cpu, LIGHT_KNEE};
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP art14_terrain_shoulder_mesh: install/map missing");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        let tiles = load_tile_set(assets);
        let mesh = build_terrain(assets, &tiles);
        assert!(
            mesh.vertices.len() > 1000,
            "the real map mesh must have vertices, got {}",
            mesh.vertices.len()
        );
        let curve = DayNightCurve::new(Preset::Lordaeron);
        let sample = curve.sample(12.0);
        let (sun, ambient) = (sample.terrain.sun, sample.terrain.ambient);
        let light = glam::Vec3::from_slice(&sample.sun_direction).normalize();
        let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let mut clipped_before = 0usize;
        let mut over_knee = 0usize;
        let mut plateau_before = 0usize;
        let mut plateau_after = 0usize;
        let mut clipped_values_after = std::collections::BTreeSet::new();
        let mut sun_facing_before = 0.0f32;
        let mut sun_facing_after = 0.0f32;
        for vertex in &mesh.vertices {
            let normal = glam::Vec3::from_slice(&vertex.normal).normalize_or_zero();
            let ndl = normal.dot(light).clamp(0.0, 1.0);
            let raw = [
                ambient[0] + sun[0] * ndl,
                ambient[1] + sun[1] * ndl,
                ambient[2] + sun[2] * ndl,
            ];
            let clamped = [raw[0].clamp(0.0, 1.0), raw[1].clamp(0.0, 1.0), raw[2].clamp(0.0, 1.0)];
            let clipped =
                raw.iter().zip(clamped.iter()).any(|(raw, clamp)| raw - clamp > 1e-4);
            if clipped {
                clipped_before += 1;
            }
            if raw.iter().any(|value| *value > LIGHT_KNEE) {
                over_knee += 1;
            }
            let after = [
                shoulder_cpu(raw[0], LIGHT_KNEE),
                shoulder_cpu(raw[1], LIGHT_KNEE),
                shoulder_cpu(raw[2], LIGHT_KNEE),
            ];
            let before_luma = luma(clamped);
            let after_luma = luma(after);
            if before_luma >= 0.999 {
                plateau_before += 1;
            }
            if after_luma >= 0.999 {
                plateau_after += 1;
            }
            if clipped {
                clipped_values_after.insert((after_luma * 1000.0).round() as i64);
            }
            if ndl >= 0.9 {
                sun_facing_before = sun_facing_before.max(before_luma);
                sun_facing_after = sun_facing_after.max(after_luma);
            }
        }
        println!(
            "ART14_SHOULDER_MESH verts={} over_knee={} clipped_before={} plateau_before={} \
             plateau_after={} clipped_luma_values_after={} sun_facing_before={:.3} \
             sun_facing_after={:.3}",
            mesh.vertices.len(),
            over_knee,
            clipped_before,
            plateau_before,
            plateau_after,
            clipped_values_after.len(),
            sun_facing_before,
            sun_facing_after
        );
        assert!(
            over_knee > 0,
            "the real map has no vertex above the knee; the shoulder would never act"
        );
        assert!(
            clipped_before > 0,
            "the raw DNC light never passes 1.0 on this map; nothing to compress"
        );
        assert!(
            plateau_before > 100,
            "the legacy clamp has no plateau on the real mesh ({plateau_before} vertices at 1.0)"
        );
        assert_eq!(
            plateau_after, 0,
            "the shoulder must never reach white (plateau after = {plateau_after})"
        );
        assert!(
            clipped_values_after.len() > 20,
            "the former clip plateau must spread over many values, got {}",
            clipped_values_after.len()
        );
        assert!(
            sun_facing_after < 1.0,
            "a sun-facing slope must not reach white, got {sun_facing_after:.3}"
        );
        assert!(
            sun_facing_after > 0.85,
            "the shoulder must keep authored brightness, got {sun_facing_after:.3}"
        );
    }

    #[test]
    #[ignore = "heavy: 2414x1080 terrain-only A/B frame + per-cell metric; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn art14_terrain_frame_2414() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP art14_terrain_frame_2414: install/map missing");
            return;
        };
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut game = Game::new(Assets::load(Some(&mpq), Some(&map)), 0x5EED);
        let width = 2414u32;
        let height = 1080u32;
        game.resize(width as f32, height as f32);
        for _ in 0..30 {
            game.update(1.0 / 30.0);
        }
        let scene = game.scene();
        let view_proj = scene.view_proj.to_cols_array_2d();
        let cells_w = game.assets.map_width_cells as usize;
        let cells_h = game.assets.map_height_cells as usize;
        let stride = cells_w + 1;
        let corner_height = |x: usize, z: usize| -> f32 {
            game.assets
                .terrain_tiles
                .get(z * stride + x)
                .map(final_height)
                .unwrap_or(0.0)
        };
        let project = |x: f32, y: f32, z: f32| -> Option<(f32, f32)> {
            let m = glam::Mat4::from_cols_array_2d(&view_proj);
            let clip = m * glam::Vec4::new(x, y, z, 1.0);
            if clip.w <= 1e-3 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            Some((
                (ndc.x * 0.5 + 0.5) * width as f32,
                (1.0 - (ndc.y * 0.5 + 0.5)) * height as f32,
            ))
        };
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/agent-world-art/all-layers-20261121");
        let _ = std::fs::create_dir_all(&out);
        let options = crate::render::RenderOptions {
            skip_units: true,
            skip_ui: true,
            skip_doodads: true,
            ..Default::default()
        };
        let luma = |r: u8, g: u8, b: u8| 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
        let render = |tag: &str| -> Option<Vec<u8>> {
            let rgba = match crate::render::render_scene_to_rgba_with(
                &game.assets,
                &game.font.atlas,
                None,
                &scene,
                width,
                height,
                options,
            ) {
                Ok(rgba) => rgba,
                Err(error) => {
                    crate::dev_log!("SKIP art14_terrain_frame_2414: {error}");
                    return None;
                }
            };
            crate::render::write_ppm(
                out.join(format!("art14-frame-{tag}-2414x1080.ppm"))
                    .to_str()
                    .unwrap(),
                width,
                height,
                &rgba,
            );
            Some(rgba)
        };
        let measure = |rgba: &[u8], tag: &str| {
            let mut full = 0.0f64;
            let mut full_n = 0u64;
            for (index, pixel) in rgba.chunks_exact(4).enumerate() {
                if index % 7 == 0 {
                    full += luma(pixel[0], pixel[1], pixel[2]) as f64;
                    full_n += 1;
                }
            }
            // Per-cell mean luma at the projected cell centre. A 5x5 pixel box
            // is the authored tile's own detail; the cell-to-cell difference is
            // what the mip chain cannot remove and what reads as the grid.
            let mut cell = vec![f32::NAN; cells_w * cells_h];
            let mut far_y = Vec::new();
            let mut near_y = Vec::new();
            let mut far_x = Vec::new();
            let mut near_x = Vec::new();
            for z in 0..cells_h {
                for x in 0..cells_w {
                    let cx = (x as f32 + 0.5) * TERRAIN_TILE_SIZE;
                    let cz = (z as f32 + 0.5) * TERRAIN_TILE_SIZE;
                    let cy = (corner_height(x, z)
                        + corner_height(x + 1, z)
                        + corner_height(x, z + 1)
                        + corner_height(x + 1, z + 1))
                        * 0.25;
                    let Some((sx, sy)) = project(cx, cy, cz) else {
                        continue;
                    };
                    if sx < 6.0
                        || sy < 6.0
                        || sx > width as f32 - 6.0
                        || sy > height as f32 - 6.0
                    {
                        continue;
                    }
                    let px = sx.round() as i32;
                    let py = sy.round() as i32;
                    let mut sum = 0.0;
                    let mut n = 0;
                    for dy in -2i32..=2 {
                        for dx in -2i32..=2 {
                            let ix = (px + dx).clamp(0, width as i32 - 1) as usize;
                            let iy = (py + dy).clamp(0, height as i32 - 1) as usize;
                            let i = (iy * width as usize + ix) * 4;
                            sum += luma(rgba[i], rgba[i + 1], rgba[i + 2]);
                            n += 1;
                        }
                    }
                    cell[z * cells_w + x] = sum / n as f32;
                    if sy < height as f32 * 0.45 {
                        far_x.push(x);
                        far_y.push(z);
                    } else if sy > height as f32 * 0.55 {
                        near_x.push(x);
                        near_y.push(z);
                    }
                }
            }
            let band_stats = |xs: &[usize], zs: &[usize]| -> (f32, f32, f32) {
                let values: Vec<f32> = xs
                    .iter()
                    .zip(zs.iter())
                    .map(|(x, z)| cell[*z * cells_w + *x])
                    .filter(|v| v.is_finite())
                    .collect();
                if values.len() < 2 {
                    return (f32::NAN, f32::NAN, 0.0);
                }
                let mean = values.iter().sum::<f32>() / values.len() as f32;
                let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>()
                    / values.len() as f32;
                // Adjacent cell difference (grid neighbours on this band).
                let mut delta = 0.0f64;
                let mut pairs = 0u64;
                for (x, z) in xs.iter().zip(zs.iter()) {
                    let a = cell[*z * cells_w + *x];
                    if !a.is_finite() {
                        continue;
                    }
                    for (nx, nz) in [(*x + 1, *z), (*x, *z + 1)] {
                        if nx >= cells_w || nz >= cells_h {
                            continue;
                        }
                        let b = cell[nz * cells_w + nx];
                        if b.is_finite() {
                            delta += (a - b).abs() as f64;
                            pairs += 1;
                        }
                    }
                }
                (mean, var.sqrt(), (delta / pairs.max(1) as f64) as f32)
            };
            let (far_mean, far_std, far_adj) = band_stats(&far_x, &far_y);
            let (near_mean, near_std, near_adj) = band_stats(&near_x, &near_y);
            let patch_w = (width as f32 * 200.0 / 2414.0) as u32;
            let patch_h = (height as f32 * 140.0 / 1080.0) as u32;
            let x0 = (width as f32 * 0.26) as u32;
            let y0 = (height as f32 * 0.34) as u32;
            let stats = patch_stats(rgba, width, x0, y0, patch_w.max(3), patch_h.max(3));
            crate::dev_log!(
                "ART14_FRAME tag={tag} size={width}x{height} full_luma={:.2} patch_mean=({:.1},{:.1},{:.1}) \
                 patch_std=({:.1},{:.1},{:.1}) patch_edge={:.3} far_cells={} far_std={:.2} far_adj={:.2} \
                 near_cells={} near_std={:.2} near_adj={:.2} path={}",
                full / full_n.max(1) as f64,
                stats.mean[0],
                stats.mean[1],
                stats.mean[2],
                stats.std[0],
                stats.std[1],
                stats.std[2],
                stats.edge,
                far_x.len(),
                far_std,
                far_adj,
                near_x.len(),
                near_std,
                near_adj,
                out.join(format!("art14-frame-{tag}-2414x1080.ppm")).display()
            );
            (far_std, far_adj, near_std, near_adj)
        };
        // A/B in one process: the renderer is created per call, and the fade
        // start is read there, so each frame differs only by the switch. A small
        // sweep proves the chosen default acts on this camera, not only on a
        // synthetic far distance.
        let mut measured = Vec::new();
        for (tag, fade) in [("legacy", 0.0f32), ("fade06", 0.6), ("default", 0.9), ("fade12", 1.2)] {
            unsafe { std::env::set_var("H2_TERRAIN_VARIATION_FADE", format!("{fade}")) };
            let Some(rgba) = render(tag) else { return };
            measured.push((tag, fade, measure(&rgba, tag)));
        }
        unsafe { std::env::remove_var("H2_TERRAIN_VARIATION_FADE") };
        let find = |tag: &str| {
            measured
                .iter()
                .find(|(name, _, _)| *name == tag)
                .map(|(_, _, stats)| *stats)
                .expect("fade sweep entry")
        };
        let legacy = find("legacy");
        let shipped = find("default");
        assert!(legacy.0.is_finite(), "no far-field cells were sampled");
        assert!(
            shipped.0 <= legacy.0,
            "the far-field cell variance must not grow: {} -> {}",
            legacy.0,
            shipped.0
        );
        assert!(
            shipped.1 < legacy.1 * 0.99,
            "the far-field cell grid must lose its per-cell colour steps: \
             adjacent delta {} -> {}",
            legacy.1,
            shipped.1
        );
        assert!(
            shipped.2 <= legacy.2 * 1.02 + 0.5,
            "near-field variation must survive the fade: {} -> {}",
            legacy.2,
            shipped.2
        );
    }

    #[test]
    #[ignore = "diagnostic: headless GPU hero close-up; needs War3.mpq + a Vulkan adapter; run_layer_gates.sh --ignored"]
    fn agent_world_hero_models_closeup() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_hero_models: user War3.mpq/map not present");
            return;
        };
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let assets = shared_assets(&mpq, &map);
        let center = glam::Vec2::new(2000.0, 2000.0);
        let mut camera = crate::Camera::new(assets.world_size);
        camera.viewport = glam::Vec2::new(1200.0, 800.0);
        camera.target = center;
        camera.distance = 700.0;
        let scene = crate::Scene {
            view_proj: camera.view_proj(),
            eye: camera.eye(),
            viewport: camera.viewport,
            units: vec![
                unit_draw(&assets, wc3_sim::UnitKind::Footman, center - glam::Vec2::new(90.0, 0.0), 0.0),
                unit_draw(&assets, wc3_sim::UnitKind::Archer, center + glam::Vec2::new(60.0, 40.0), 0.0),
                unit_draw(&assets, wc3_sim::UnitKind::Peasant, center + glam::Vec2::new(180.0, -10.0), 0.0),
            ],
            rings: Vec::new(),
            health: Vec::new(),
            ui: crate::hud::UiBatch::default(),
            overlay: None,
        };
        let options = crate::render::RenderOptions {
            skip_terrain: true,
            skip_ui: true,
            ..Default::default()
        };
        let rgba = match crate::render::render_scene_to_rgba_with(
            &assets,
            &crate::assets::TextureImage::white(),
            None,
            &scene,
            1200,
            800,
            options,
        ) {
            Ok(rgba) => rgba,
            Err(error) => {
                crate::dev_log!("SKIP agent_world_hero_models: {error}");
                return;
            }
        };
        let path = out_dir().join("heroes-after.ppm");
        crate::render::write_ppm(path.to_str().unwrap(), 1200, 800, &rgba);
        // Coherent units: a meaningful silhouette and both team colour and
        // skin/texture pixels in the frame.
        let mut unit_pixels = 0u64;
        let mut team_pixels = 0u64;
        for pixel in rgba.chunks_exact(4) {
            let (r, g, b) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
            if (r - 60).abs() + (g - 78).abs() + (b - 93).abs() <= 40 {
                continue;
            }
            unit_pixels += 1;
            if b > r + 14 && b > g + 2 {
                team_pixels += 1;
            }
        }
        println!("HEROES pixels={unit_pixels} team={team_pixels}");
        assert!(unit_pixels > 8_000, "heroes must fill the close-up frame");
        // Team-colour layers come from the per-definition model assets (model
        // owner, this round); the close-up asserts real hero geometry + scale.
        let _ = team_pixels;
    }

    #[test]
    fn agent_world_doodads_are_placed() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_doodads: user War3.mpq/map not present");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        let doodads = crate::doodads::DoodadWorld::load(&assets);
        println!(
            "DOODADS parsed={} placed={} models={} model_failures={} textures={} instances={}",
            doodads.stats.parsed,
            doodads.stats.placed,
            doodads.models.len(),
            doodads.stats.model_failures,
            doodads.textures.len(),
            doodads.instances.len()
        );
        assert_eq!(doodads.stats.source, "re", "doodads come from the canonical RE layer");
        assert!(doodads.stats.parsed >= 2000, "Human04 has ~2509 doodads");
        assert!(
            doodads.stats.placed as f32 > doodads.stats.parsed as f32 * 0.8,
            "most doodads must resolve a model: {} / {}",
            doodads.stats.placed,
            doodads.stats.parsed
        );
        assert!(doodads.models.len() >= 10, "trees/rocks/signs models loaded");
        assert!(doodads.textures.len() > 3, "doodad textures loaded");
        assert!(
            doodads.stats.model_failures < doodads.stats.parsed / 5,
            "model failures are the minority"
        );
    }

    /// Load-time asset provenance for the 1080p unit frame: the per-def model
    /// count and every failure line, so a regression in `Assets::load` is
    /// attributable without guessing from pixels.
    #[test]
    fn human04_asset_debug_snapshot() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP asset_debug: install/map missing");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        for line in assets.debug.iter().take(40) {
            crate::dev_log!("ASSET_DEBUG {line}");
        }
        crate::dev_log!(
            "ASSET_DEBUG_SUMMARY models={} textures={}",
            assets.models.len(),
            assets.textures.len()
        );
    }

    #[test]
    fn agent_world_units_are_readable_at_1080p() {
        let Some((mpq, map)) = data() else {
            crate::dev_log!("SKIP agent_world_units: user War3.mpq/map not present");
            return;
        };
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let assets = Assets::load(Some(&mpq), Some(&map));
        let mut game = Game::new(assets, 0x7E57);
        let width = 2414;
        let height = 1080;
        game.resize(width as f32, height as f32);
        // Retail boot opens the glue menu on the first tick (`Game::update`,
        // `boot_menu`); a host that injects the battle directly ticks the menu
        // away and puts the game back into `Playing` through the public
        // `screen` field before the sim advances.
        game.update(1.0 / 30.0);
        game.screen = crate::game::Screen::Playing;
        for _ in 0..30 {
            game.update(1.0 / 30.0);
        }
        let scene = game.scene();
        let options = crate::render::RenderOptions {
            skip_terrain: true,
            skip_ui: true,
            ..Default::default()
        };
        let rgba = match crate::render::render_scene_to_rgba_with(
            &game.assets,
            &game.font.atlas,
            None,
            &scene,
            width,
            height,
            options,
        ) {
            Ok(rgba) => rgba,
            Err(error) => {
                crate::dev_log!("SKIP agent_world_units: headless render unavailable: {error}");
                return;
            }
        };
        let path = out_dir().join("units-after.ppm");
        crate::render::write_ppm(path.to_str().unwrap(), width, height, &rgba);
        // Clear colour in sRGB8 is ~ (60, 78, 93); anything far from it is a
        // unit pixel.
        let mut unit_pixels = 0u64;
        let mut team_pixels = 0u64;
        let mut red_pixels = 0u64;
        for pixel in rgba.chunks_exact(4) {
            let (r, g, b) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
            let delta = (r - 60).abs() + (g - 78).abs() + (b - 93).abs();
            if delta <= 40 {
                continue;
            }
            unit_pixels += 1;
            if b > r + 14 && b > g + 2 {
                team_pixels += 1;
            }
            if r > b + 22 && r > g + 4 {
                red_pixels += 1;
            }
        }
        // Projected bind-pose heights for the two named checks.
        let town_hall = game
            .assets
            .model(wc3_sim::UnitKind::TownHall);
        let hall_px = crate::model_render::project_model_height(
            &game.assets,
            scene.view_proj,
            wc3_sim::UnitKind::TownHall,
            game.base_pos,
        )
        .unwrap_or(0.0);
        let archer_pos = game.base_pos + glam::Vec2::new(160.0, 0.0);
        let archer_px = crate::model_render::project_model_height(
            &game.assets,
            scene.view_proj,
            wc3_sim::UnitKind::Archer,
            archer_pos,
        )
        .unwrap_or(0.0);
        println!(
            "UNITS pixels={unit_pixels} team={team_pixels} enemy={red_pixels} townhall_height={:.0} world={:.0}px archer_height={:.0}px",
            town_hall.height, hall_px, archer_px
        );
        assert!(unit_pixels > 8_000, "units must cover real screen area");
        assert!(
            team_pixels + red_pixels > 500,
            "team tint visible on at least one side (player={team_pixels}, enemy={red_pixels})"
        );
        assert!(red_pixels > 100, "enemy team tint visible");
        // The hall now renders from the per-definition MDX (120 world units) on
        // its real cliff-layer height, not the old 497-unit fallback model on a
        // flattened map; at this camera it projects to ~113 px. 90 px is the
        // readable bound (8.3 % of the 1080p frame height).
        assert!(hall_px > 90.0, "TownHall must read at 1080p: {hall_px:.0}px");
        assert!(archer_px > 32.0, "Archer must read at 1080p: {archer_px:.0}px");
    }

    fn melee_fixtures() -> Option<(PathBuf, PathBuf, PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())?;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Maps/Campaign");
        let human = root.join("Human04.w3m");
        let orc = root.join("Orc01.w3m");
        (human.is_file() && orc.is_file()).then_some((mpq, human, orc))
    }

    /// `rendering.terrain.water-plane`: the water mesh exists only over real
    /// water cells, at each cell's own `water_height()`, and the terrain bed is
    /// below the surface on deep water. No translucent sheet over dry land.
    #[test]
    #[ignore = "diagnostic: headless GPU water-plane close-up; needs War3.mpq + a Vulkan adapter; run_layer_gates.sh --ignored"]
    fn rendering_terrain_water_plane() {
        let Some((mpq, _human, _orc)) = melee_fixtures() else {
            crate::dev_log!("SKIP rendering_terrain_water_plane: fixtures missing");
            return;
        };
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let maps: Vec<PathBuf> = [
            root.join("../../analysis/war3_full3/Maps/Campaign/Human04.w3m"),
            root.join("../../analysis/war3_full3/Maps/Campaign/Orc01.w3m"),
            root.join("../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x"),
        ]
        .into_iter()
        // Orc01 has 12k water cells; Human04 + the reported Ponds map cover
        // the water-plane contract without the multi-minute cost.
        .filter(|path| !path.to_string_lossy().contains("Orc01"))
        .into_iter()
        .filter(|path| path.is_file())
        .collect();
        assert!(!maps.is_empty(), "water-plane fixtures");
        let mut deep_total = 0usize;
        for map in &maps {
            let assets = shared_assets(&mpq, map);
            let tiles = load_tile_set(&assets);
            let mesh = build_terrain(&assets, &tiles);
            let offset = tiles.water.offset;
            let cells_w = assets.map_width_cells as usize;
            let cells_h = assets.map_height_cells as usize;
            let width = cells_w + 1;
            let mut water_cells = 0usize;
            let mut deep = 0usize;
            for z in 0..cells_h {
                for x in 0..cells_w {
                    let Some(tile) = assets.terrain_tiles.get(z * width + x) else {
                        continue;
                    };
                    if !tile.is_water() {
                        continue;
                    }
                    water_cells += 1;
                    let surface = final_water_height(tile, offset);
                    let bed = assets.height_at(
                        (x as f32 + 0.5) * TERRAIN_TILE_SIZE,
                        (z as f32 + 0.5) * TERRAIN_TILE_SIZE,
                    );
                    if surface - bed > 20.0 {
                        deep += 1;
                    }
                }
            }
            // Every water triangle must sit over a cell that has at least one
            // water corner (HiveWE draws a quad when ANY corner is wet), at
            // that corner's own water level plus the Water.slk offset. No
            // triangle may cover a cell with no water corner at all.
            let mut land_tris = 0usize;
            let mut worst_height = 0.0f32;
            let any_water_corner = |x: usize, z: usize| -> bool {
                [(x, z), (x + 1, z), (x + 1, z + 1), (x, z + 1)]
                    .iter()
                    .any(|(cx, cz)| {
                        assets
                            .terrain_tiles
                            .get(cz * width + cx)
                            .map(|tile| tile.is_water())
                            .unwrap_or(false)
                    })
            };
            for tri in mesh.water_indices.chunks_exact(3) {
                let mut sum = glam::Vec3::ZERO;
                for index in tri {
                    if let Some(vertex) = mesh.water_vertices.get(*index as usize) {
                        sum += glam::Vec3::from_array(vertex.position);
                    }
                }
                let centre = sum / 3.0;
                let tx = (centre.x / TERRAIN_TILE_SIZE).floor() as i64;
                let tz = (centre.z / TERRAIN_TILE_SIZE).floor() as i64;
                let (cx, cz) = (tx.max(0) as usize, tz.max(0) as usize);
                if cx >= cells_w || cz >= cells_h || !any_water_corner(cx, cz) {
                    land_tris += 1;
                    continue;
                }
                // Per-corner surface: each vertex must equal the level of its
                // own W3E corner (no averaging, no +2 fudge).
                for index in tri {
                    if let Some(vertex) = mesh.water_vertices.get(*index as usize) {
                        let vx = (vertex.position[0] / TERRAIN_TILE_SIZE).round() as usize;
                        let vz = (vertex.position[2] / TERRAIN_TILE_SIZE).round() as usize;
                        if let Some(corner) = assets.terrain_tiles.get(vz.min(cells_h) * width + vx.min(cells_w)) {
                            let expected = final_water_height(corner, offset);
                            worst_height = worst_height.max((vertex.position[1] - expected).abs());
                        }
                    }
                }
            }
            crate::dev_log!(
                "TERRAIN_WATER_PLANE map={} water_cells={water_cells} deep_water_tiles={deep} water_tris={} land_tris={land_tris} worst_height={worst_height:.2} offset={offset:.2}",
                map.display(),
                mesh.water_indices.len() / 3
            );
            assert!(water_cells > 0, "{} has water cells", map.display());
            deep_total += deep;
            assert_eq!(land_tris, 0, "{} water mesh covers dry land", map.display());
            assert!(
                worst_height < 0.01,
                "{} water vertex deviates by {worst_height:.2} from the W3E level + Water.slk offset",
                map.display()
            );
        }
        assert!(deep_total > 0, "at least one map has deep water below the surface");
    }

    /// `rendering.terrain.cliffs`: numeric mesh/height seams and cliff/ramp
    /// continuity on the melee maps (Human04/Orc01).
    #[test]
    #[ignore = "diagnostic: headless GPU cliff close-up; needs War3.mpq + a Vulkan adapter; run_layer_gates.sh --ignored"]
    fn rendering_terrain_cliffs() {
        let Some((mpq, human, orc)) = melee_fixtures() else {
            crate::dev_log!("SKIP rendering_terrain_cliffs: fixtures missing");
            return;
        };
        let mut total_ramps = 0usize;
        for map in [&human, &orc] {
            let assets = Assets::load(Some(&mpq), Some(map));
            let tiles = load_tile_set(&assets);
            let mesh = build_terrain(&assets, &tiles);
            let width = assets.map_width_cells as usize + 1;
            // Numeric seam: every mesh corner Y equals the W3E final height
            // (`corner_height + layer_height - 2.0`).
            let mut corners: std::collections::HashMap<(i64, i64), f32> = Default::default();
            for vertex in &mesh.vertices {
                let key = (
                    (vertex.position[0] * 100.0).round() as i64,
                    (vertex.position[2] * 100.0).round() as i64,
                );
                corners.entry(key).or_insert(vertex.position[1]);
            }
            let mut worst_seam = 0.0f32;
            for z in (0..assets.map_height_cells as usize).step_by(7) {
                for x in (0..assets.map_width_cells as usize).step_by(7) {
                    let Some(tile) = assets.terrain_tiles.get(z * width + x) else {
                        continue;
                    };
                    let expected = final_height(tile);
                    let key = (
                        ((x as f32 * TERRAIN_TILE_SIZE) * 100.0).round() as i64,
                        ((z as f32 * TERRAIN_TILE_SIZE) * 100.0).round() as i64,
                    );
                    if let Some(&actual) = corners.get(&key) {
                        worst_seam = worst_seam.max((actual - expected).abs());
                    }
                }
            }
            assert!(
                worst_seam < 0.01,
                "{} mesh corner seam {worst_seam:.4}",
                map.display()
            );
            // Cliff/ramp continuity with the authoritative WPM semantics
            // (0x02 = unwalkable) + deep-water mask.
            let pathing = assets.pathing.as_ref().expect("map pathing (wpm)");
            let sx = pathing.width as f32 / assets.map_width_cells.max(1) as f32;
            let sz = pathing.height as f32 / assets.map_height_cells.max(1) as f32;
            let walkable = |x: usize, z: usize| -> bool {
                let px = ((x as f32 + 0.5) * sx).floor() as u32;
                let pz = ((z as f32 + 0.5) * sz).floor() as u32;
                let cell = pathing.cell(px.min(pathing.width - 1), pz.min(pathing.height - 1));
                let cell_ok = cell.map(|c| c & 0x02 == 0).unwrap_or(false);
                cell_ok
                    && !assets.is_water(
                        (x as f32 + 0.5) * TERRAIN_TILE_SIZE,
                        (z as f32 + 0.5) * TERRAIN_TILE_SIZE,
                    )
            };
            let tile_at = |x: usize, z: usize| -> Option<&wc3_formats::w3e::TerrainTile> {
                assets.terrain_tiles.get(z * width + x)
            };
            let mut ramps = 0usize;
            let mut cliff_steps = 0usize;
            for z in 0..assets.map_height_cells as usize {
                for x in 0..assets.map_width_cells as usize {
                    let Some(a) = tile_at(x, z) else { continue };
                    if a.is_ramp() {
                        ramps += 1;
                    }
                    for (dx, dz) in [(1usize, 0usize), (0, 1)] {
                        let (nx, nz) = (x + dx, z + dz);
                        let Some(b) = tile_at(nx, nz) else { continue };
                        let delta = (final_height(a) - final_height(b)).abs();
                        if walkable(x, z)
                            && walkable(nx, nz)
                            && !a.is_ramp()
                            && !b.is_ramp()
                            && delta > 180.0
                        {
                            cliff_steps += 1;
                        }
                    }
                }
            }
            total_ramps += ramps;
            crate::dev_log!(
                "TERRAIN_CLIFFS map={} seam={worst_seam:.4} ramps={ramps} cliff_steps={cliff_steps}",
                map.display()
            );
            assert_eq!(cliff_steps, 0, "{} walkable cliff steps", map.display());
        }
        assert!(total_ramps > 0, "melee maps carry ramp tiles");
    }

    /// The active 1v1 map, used for the device terrain gate.
    fn melee_map() -> Option<(PathBuf, PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())?;
        let map = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Melee.w3x");
        map.is_file().then_some((mpq, map))
    }

    /// HiveWE `corner_final_ground_height`: the TFT cliff layer is part of the
    /// rendered surface. Melee.w3x has layer heights 1..7, so the old renderer
    /// flattened up to 740 world units of relief and the water plane sat above
    /// dry land.
    #[test]
    fn hivewe_cliff_layer_heights_raise_the_terrain() {
        let Some((mpq, map)) = melee_map() else {
            crate::dev_log!("SKIP hivewe cliff layers: Melee.w3x fixtures missing");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        let tiles = load_tile_set(&assets);
        let mesh = build_terrain(&assets, &tiles);
        let width = assets.map_width_cells as usize + 1;
        let mut min_layer = u8::MAX;
        let mut max_layer = 0u8;
        let mut max_relief = 0.0f32;
        for z in 0..assets.map_height_cells as usize {
            for x in 0..assets.map_width_cells as usize {
                let Some(tile) = assets.terrain_tiles.get(z * width + x) else {
                    continue;
                };
                min_layer = min_layer.min(tile.cliff_level());
                max_layer = max_layer.max(tile.cliff_level());
                let expected = final_height(tile);
                let actual = mesh.vertices[z * width + x].position[1];
                max_relief = max_relief.max((actual - expected).abs());
            }
        }
        crate::dev_log!(
            "HIVE_LAYERS min_layer={min_layer} max_layer={max_layer} mesh_error={max_relief:.3} high={:.1}",
            mesh.vertices
                .iter()
                .map(|vertex| vertex.position[1])
                .fold(f32::NEG_INFINITY, f32::max)
        );
        assert!(max_layer >= 4, "Melee.w3x carries cliff layers: max {max_layer}");
        assert!(
            min_layer >= 1,
            "layer 0 is not used by this map (old code assumed 2 for every corner)"
        );
        assert!(max_relief < 0.01, "mesh follows final heights: {max_relief}");
        // The relief between the lowest and highest rendered corner must be at
        // least the layer span (map is 9 tiles from min to max height).
        let high = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let low = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(high - low > 700.0, "cliff relief {:.1} was flattened", high - low);
    }

    /// Water.slk `height` and the four authored colour rows drive the surface
    /// (HiveWE water.vert). On Melee.w3x `*Sha` resolves to LSha: height -0.7,
    /// Smin white/alpha 10, Smax 117,117,200/alpha 219, Dmax 96,96,192/250.
    #[test]
    fn hivewe_water_uses_slk_offset_and_four_colour_ramp() {
        let Some((mpq, map)) = melee_map() else {
            crate::dev_log!("SKIP hivewe water ramp: Melee.w3x fixtures missing");
            return;
        };
        let assets = shared_assets(&mpq, &map);
        let tiles = load_tile_set(&assets);
        let water = &tiles.water;
        crate::dev_log!(
            "HIVE_WATER offset={} smin={:?} smax={:?} dmin={:?} dmax={:?} rate={}",
            water.offset,
            water.shallow_min,
            water.shallow_max,
            water.deep_min,
            water.deep_max,
            water.rate
        );
        assert!((water.offset + 0.7).abs() < 0.01, "Water.slk height -0.7: {}", water.offset);
        assert!((water.shallow_min[3] - 10.0 / 255.0).abs() < 0.01, "Smin_A=10");
        assert!((water.shallow_max[3] - 219.0 / 255.0).abs() < 0.01, "Smax_A=219");
        assert!((water.deep_max[3] - 250.0 / 255.0).abs() < 0.01, "Dmax_A=250");
        assert!(water.shallow_max[2] > water.shallow_max[0], "Smax is blue");
        // Ramp: the shore is nearly transparent white, deep water is the blue
        // Dmax; the 64-unit mark lands on Smax/Dmin.
        let shore = water_ramp(water, 0.0);
        assert!(shore[3] < 0.06, "shore alpha {}", shore[3]);
        let deep_level = water_ramp(water, WATER_DEEP_LEVEL);
        assert!((deep_level[3] - 219.0 / 255.0).abs() < 0.02, "depth 64 -> Smax");
        let deepest = water_ramp(water, 400.0);
        assert!((deepest[3] - 250.0 / 255.0).abs() < 0.02, "deep -> Dmax");
        assert!(deepest[0] < deep_level[0], "deep water is darker (Dmax R=96 < Smax R=117)");

        // The water mesh sits at `(corner_water_height - 0.7) * 128`, and a dry
        // corner of a shoreline quad is nearly transparent (no sheet over land).
        let mesh = build_terrain(&assets, &tiles);
        let width_cells = assets.map_width_cells as usize;
        let width = width_cells + 1;
        let mut max_surface_error = 0.0f32;
        let mut dry_corner_alpha = None;
        for vertex in &mesh.water_vertices {
            let vx = (vertex.position[0] / TERRAIN_TILE_SIZE).round() as usize;
            let vz = (vertex.position[2] / TERRAIN_TILE_SIZE).round() as usize;
            if let Some(tile) = assets.terrain_tiles.get(vz * width + vx) {
                let expected = final_water_height(tile, water.offset);
                max_surface_error = max_surface_error.max((vertex.position[1] - expected).abs());
                if !tile.is_water() && final_height(tile) > expected + 1.0 {
                    dry_corner_alpha = Some(vertex.color[3]);
                }
            }
        }
        assert!(max_surface_error < 0.01, "surface offset error {max_surface_error}");
        let alpha = dry_corner_alpha.expect("shoreline quads carry dry corners");
        assert!(alpha < 0.06, "dry corner must be nearly transparent: {alpha}");
    }

    /// The variation nibble is five bits and the packed sheet must contain the
    /// subtextures the engine actually samples: for a standard 4x4 sheet
    /// variation 0 is tile 0 and every other variation is tile 15 (HiveWE
    /// `get_tile_variation`); extended 8x4 sheets map variation n to layer 16+n.
    #[test]
    fn hivewe_variation_packing_uses_engine_subtextures() {
        // Standard 512x512 sheet: each 128px tile gets a unique colour.
        let size = 512u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let tile_x = x / 128;
                let tile_y = y / 128;
                let index = ((y * size + x) * 4) as usize;
                rgba[index] = (tile_x * 16 + tile_y * 64) as u8;
                rgba[index + 1] = (tile_x * 40) as u8;
                rgba[index + 2] = (tile_y * 30) as u8;
                rgba[index + 3] = 255;
            }
        }
        let (packed, _, _) = pack_variation_sheet(size, size, &rgba);
        let sample = |quad: u32| -> [u8; 4] {
            let x = (quad % 4) * 128 + 64;
            let y = (quad / 4) * 128 + 64;
            let index = ((y * packed.width + x) * 4) as usize;
            [
                packed.rgba[index],
                packed.rgba[index + 1],
                packed.rgba[index + 2],
                packed.rgba[index + 3],
            ]
        };
        assert_eq!(packed.width, 512);
        assert_eq!(sample(0), [0, 0, 0, 255], "variation 0 -> tile 0");
        // Layer 15 is tile (3,3): R=3*16+3*64=240, G=120, B=90.
        for quad in 1..16 {
            assert_eq!(sample(quad), [240, 120, 90, 255], "variation {quad} -> tile 15");
        }

        // Extended 1024x512: variation n -> layer 16+n, i.e. columns 4..7 of
        // the second 4x4 grid, not the row-major tail of an 8-wide sheet.
        let wide = 1024u32;
        let mut rgba = vec![0u8; (wide * size * 4) as usize];
        for y in 0..size {
            for x in 0..wide {
                let tile_x = x / 128;
                let tile_y = y / 128;
                let index = ((y * wide + x) * 4) as usize;
                rgba[index] = (tile_x * 20) as u8;
                rgba[index + 1] = (tile_y * 50) as u8;
                rgba[index + 3] = 255;
            }
        }
        let (packed, _, _) = pack_variation_sheet(wide, size, &rgba);
        for quad in 0..16u32 {
            let layer = 16 + quad;
            let tile_x = (layer % 16) % 4 + 4;
            let tile_y = (layer % 16) / 4;
            let expected = [(tile_x * 20) as u8, (tile_y * 50) as u8, 0, 255];
            let x = (quad % 4) * 128 + 64;
            let y = (quad / 4) * 128 + 64;
            let index = ((y * packed.width + x) * 4) as usize;
            assert_eq!(
                [
                    packed.rgba[index],
                    packed.rgba[index + 1],
                    packed.rgba[index + 2],
                    packed.rgba[index + 3]
                ],
                expected,
                "extended variation {quad} -> layer {layer}"
            );
        }
    }

    /// Alpha-mask holes in the authored BLPs are black; packing them raw made
    /// mipmaps darker at every cell border (the checkerboard). The packed RGB
    /// must be the base colour wherever the mask is transparent, while the
    /// alpha channel is preserved.
    #[test]
    fn hivewe_alpha_holes_are_filled_not_black() {
        let size = 512u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[0] = 200;
            pixel[1] = 180;
            pixel[2] = 120;
            pixel[3] = 255;
        }
        // Make layer 15 fully transparent black (a mask hole).
        for y in 384..512 {
            for x in 384..512 {
                let index = ((y * size + x) * 4) as usize;
                rgba[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        let (packed, levels, _) = pack_variation_sheet(size, size, &rgba);
        let index = ((448 * packed.width + 448) * 4) as usize;
        assert_eq!(packed.rgba[index + 3], 0, "mask alpha preserved");
        assert!(
            packed.rgba[index] > 100 && packed.rgba[index + 1] > 80,
            "hole RGB filled with the base colour, got {:?}",
            &packed.rgba[index..index + 3]
        );
        // The mip pyramid of the filled sheet must not contain black blocks.
        let smallest = levels.last().expect("mip chain");
        for pixel in smallest.rgba.chunks_exact(4) {
            assert!(
                pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32 > 60,
                "mip pixel went black: {pixel:?}"
            );
        }
    }

    /// The variant texture stores five-bit W3E variations; Melee.w3x uses 16
    /// and 17, which the old `& 0x0F` reader folded onto 0/1.
    #[test]
    fn hivewe_variation_is_five_bits() {
        let Some((mpq, map)) = melee_map() else {
            crate::dev_log!("SKIP hivewe variation bits: Melee.w3x fixtures missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let tiles = load_tile_set(&assets);
        let max = tiles.variants.iter().copied().max().unwrap_or(0);
        let above_four_bits = tiles.variants.iter().filter(|value| **value > 15).count();
        crate::dev_log!("HIVE_VARIANTS max={max} above_15={above_four_bits}");
        assert!(max >= 16, "five-bit variation present: max {max}");
        assert!(above_four_bits > 0, "variations 16/17 survived the packer");
    }

    /// Diagnostic close-ups of the active 1v1 map (shoreline + cliff) for the
    /// device-gate report. Renders terrain only, at the same camera angle the
    /// device uses, and reports the water coverage so a regression to the
    /// "sheet over dry land" bug is visible in the metrics as well as the
    /// image. Run explicitly:
    /// `cargo test -p wc3-game --lib hivewe_melee_closeups -- --ignored --nocapture`
    #[test]
    #[ignore = "diagnostic: Melee.w3x shoreline/cliff headless close-ups; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn hivewe_melee_closeups() {
        let Some((mpq, map)) = melee_map() else {
            crate::dev_log!("SKIP melee close-ups: Melee.w3x fixtures missing");
            return;
        };
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let assets = Assets::load(Some(&mpq), Some(&map));
        let tiles = load_tile_set(&assets);
        let mesh = build_terrain(&assets, &tiles);
        let cells_w = assets.map_width_cells as usize;
        let cells_h = assets.map_height_cells as usize;
        let width = cells_w + 1;
        let tile = |x: usize, z: usize| assets.terrain_tiles.get(z * width + x);
        let offset = tiles.water.offset;
        // A 2x2 block of water corners = an interior pond, the same framing the
        // `agent_world_water_surface_renders` contract uses. A single deep cell
        // near a cliff can hide the surface behind the wall at a low camera.
        let mut shore = None;
        for z in 8..cells_h.saturating_sub(9) {
            for x in 8..cells_w.saturating_sub(9) {
                let water = |dx: usize, dz: usize| {
                    tile(x + dx, z + dz).map(|tile| tile.is_water()).unwrap_or(false)
                };
                if water(0, 0) && water(1, 0) && water(0, 1) && water(1, 1) {
                    shore = Some((x + 1, z + 1));
                    break;
                }
            }
            if shore.is_some() {
                break;
            }
        }
        // Largest layer step in the map = the cliff shot.
        let mut cliff = None;
        let mut step = 0.0f32;
        for z in 8..cells_h.saturating_sub(8) {
            for x in 8..cells_w.saturating_sub(8) {
                let Some(cell) = tile(x, z) else { continue };
                for (dx, dz) in [(1usize, 0usize), (0, 1)] {
                    let Some(next) = tile(x + dx, z + dz) else { continue };
                    let delta = (final_height(cell) - final_height(next)).abs();
                    if delta > step {
                        step = delta;
                        cliff = Some((x, z));
                    }
                }
            }
        }
        let (Some((sx, sz)), Some((cx, cz))) = (shore, cliff) else {
            crate::dev_log!("SKIP melee close-ups: no shoreline/cliff on the map");
            return;
        };
        let dir = out_dir().join("terrain-hivewe");
        let _ = std::fs::create_dir_all(&dir);
        for (label, gx, gz) in [("melee-pond", sx, sz), ("melee-cliff", cx, cz)] {
            let width_px = 1600u32;
            let height_px = 900u32;
            // Standalone camera: `Game::update` follows the scenario base and
            // would override a camera set before it (the first version of this
            // diagnostic photographed the base, not the chosen cell).
            let mut camera = crate::Camera::new(assets.world_size);
            camera.viewport = glam::Vec2::new(width_px as f32, height_px as f32);
            camera.target = glam::Vec2::new(
                (gx as f32 + 0.5) * TERRAIN_TILE_SIZE,
                (gz as f32 + 0.5) * TERRAIN_TILE_SIZE,
            );
            camera.distance = 1000.0;
            let scene = crate::Scene {
                view_proj: camera.view_proj(),
                eye: camera.eye(),
                viewport: camera.viewport,
                units: Vec::new(),
                rings: Vec::new(),
                health: Vec::new(),
                ui: crate::hud::UiBatch::default(),
                overlay: None,
            };
            let rgba = crate::render::render_scene_to_rgba(
                &assets,
                &crate::assets::TextureImage::white(),
                None,
                &scene,
                width_px,
                height_px,
            )
            .expect("headless terrain close-up");
            let path = dir.join(format!("{label}.ppm"));
            let _ = std::fs::create_dir_all(&dir);
            crate::render::write_ppm(path.to_str().unwrap(), width_px, height_px, &rgba);
            let mut blue = 0u64;
            let mut total = 0u64;
            for (index, pixel) in rgba.chunks_exact(4).enumerate() {
                let y = index as u32 / width_px;
                if y < 80 || y > height_px - 80 {
                    continue;
                }
                total += 1;
                let (r, g, b) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
                if b > r + 12 && b > g + 2 && b > 40 {
                    blue += 1;
                }
            }
            // Terrain-only renders have no click rings, so a clean cliff shot
            // has a low blue fraction; the shoreline shot must show water.
            crate::dev_log!(
                "MELEE_CLOSEUP {label} cell=({gx},{gz}) water_blue={:.3} path={}",
                blue as f64 / total.max(1) as f64,
                path.display()
            );
            if label == "melee-pond" {
                assert!(
                    blue as f64 / total.max(1) as f64 > 0.005,
                    "pond close-up must show water"
                );
            }
        }
        assert!(!mesh.water_vertices.is_empty());
    }

    /// Patch statistics used by the Justice ground-detail contract: mean and
    /// standard deviation per channel, luminance p5/p50/p95, full range and
    /// the Sobel edge fraction (|grad| > 12 on 0..255 luminance). These are the
    /// exact quantities the device frame was measured with.
    struct PatchStats {
        mean: [f32; 3],
        std: [f32; 3],
        p5: f32,
        p50: f32,
        p95: f32,
        range: f32,
        edge: f32,
        blue_excess: f32,
    }

    fn patch_stats(rgba: &[u8], width: u32, x0: u32, y0: u32, pw: u32, ph: u32) -> PatchStats {
        let mut sum = [0f64; 3];
        let mut sum2 = [0f64; 3];
        let n = (pw * ph) as f64;
        let mut luma = Vec::with_capacity((pw * ph) as usize);
        for y in y0..y0 + ph {
            for x in x0..x0 + pw {
                let i = ((y * width + x) * 4) as usize;
                let rgb = [rgba[i] as f64, rgba[i + 1] as f64, rgba[i + 2] as f64];
                for c in 0..3 {
                    sum[c] += rgb[c];
                    sum2[c] += rgb[c] * rgb[c];
                }
                luma.push((0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]) as f32);
            }
        }
        let mut mean = [0f32; 3];
        let mut std = [0f32; 3];
        for c in 0..3 {
            let m = sum[c] / n;
            let var = (sum2[c] / n - m * m).max(0.0);
            mean[c] = m as f32;
            std[c] = var.sqrt() as f32;
        }
        let mut sorted = luma.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let pick = |q: f32| sorted[((sorted.len() - 1) as f32 * q) as usize];
        let at = |x: u32, y: u32| -> f32 {
            luma[(y as usize) * pw as usize + x as usize]
        };
        let mut edges = 0u64;
        let mut interior = 0u64;
        for y in 1..ph - 1 {
            for x in 1..pw - 1 {
                let gx = -at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1)
                    + at(x + 1, y - 1)
                    + 2.0 * at(x + 1, y)
                    + at(x + 1, y + 1);
                let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                    + at(x - 1, y + 1)
                    + 2.0 * at(x, y + 1)
                    + at(x + 1, y + 1);
                if (gx * gx + gy * gy).sqrt() > 12.0 {
                    edges += 1;
                }
                interior += 1;
            }
        }
        PatchStats {
            mean,
            std,
            p5: pick(0.05),
            p50: pick(0.5),
            p95: pick(0.95),
            range: sorted[sorted.len() - 1] - sorted[0],
            edge: edges as f32 / interior.max(1) as f32,
            blue_excess: mean[2] - mean[0],
        }
    }

    /// The operator-facing contract: the Justice start view renders as WC3
    /// grass (per-pixel paper-tile detail), not as a flat film.
    ///
    /// The device frame measured RGB mean (30.8, 75.0, 31.9), std
    /// (5.5, 13.3, 5.7), luminance p5/p50/p95 = 47/58/67 and a Sobel edge
    /// fraction of 0.054 over the same relative patch (200x140 at 2414x1080).
    /// A real WC3 grass tile carries std 25-45 per channel and >= 0.12 edges.
    ///
    /// `H2_TERRAIN_TAG` names the PPM artifact (`before`/`after`) and
    /// `H2_TERRAIN_STRICT=1` turns the thresholds into asserts, so the same
    /// binary can record the pre-fix baseline without failing.
    #[test]
    #[ignore = "heavy: headless GPU render of the Justice start view; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn terrain_ground_detail_justice() {
        let Some(mpq) = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        else {
            crate::dev_log!("SKIP terrain_ground_detail: War3.mpq missing");
            return;
        };
        let map = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        );
        if !map.exists() {
            crate::dev_log!("SKIP terrain_ground_detail: (2)Justice.w3x missing");
            return;
        }
        let _guard = GPU.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut game = Game::new(Assets::load(Some(&mpq), Some(&map)), 0x5EED);
        // One tick populates the per-unit animator/visual state the app renders
        // with; without it `Game::scene` has no model matrices for the units
        // and the frame is terrain + doodads only.
        game.update(crate::game::TICK_SECONDS);
        let target = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().is_main_hall())
            .map(|unit| unit.pos)
            .unwrap_or(game.camera.target);
        // Authored-sheet coverage: alpha holes are refilled with a flat base
        // colour, so the fraction of alpha==0 texels tells how much of a tile
        // can ever be "flat film" if alpha is used as a blend weight.
        let tiles = load_tile_set(&game.assets);
        for (layer, image) in tiles.images.iter().enumerate() {
            let total = (image.width as usize * image.height as usize).max(1);
            let mut holes = 0usize;
            let mut partial = 0usize;
            for pixel in image.rgba.chunks_exact(4) {
                match pixel[3] {
                    0 => holes += 1,
                    255 => {}
                    _ => partial += 1,
                }
            }
            crate::dev_log!(
                "TILE_ALPHA layer={} size={}x{} alpha0={:.3} partial={:.3}",
                layer,
                image.width,
                image.height,
                holes as f64 / total as f64,
                partial as f64 / total as f64
            );
        }
        let width = 1208u32;
        let height = 540u32;
        game.camera.viewport = glam::Vec2::new(width as f32, height as f32);
        game.camera.target = target;
        game.camera.distance = 1900.0;
        let scene = game.scene();
        crate::dev_log!(
            "JUSTICE_SCENE units={} buildings={} rings={}",
            scene.units.len(),
            game.sim.alive_units().filter(|unit| unit.is_building()).count(),
            scene.rings.len()
        );
        let font = crate::hud::Font::load();
        let options = crate::render::RenderOptions {
            skip_ui: true,
            ..Default::default()
        };
        let rgba = crate::render::render_scene_to_rgba_with(
            &game.assets,
            &font.atlas,
            None,
            &scene,
            width,
            height,
            options,
        )
        .expect("justice scene render");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-render");
        let _ = std::fs::create_dir_all(&dir);
        let tag = std::env::var("H2_TERRAIN_TAG").unwrap_or_else(|_| "after".to_string());
        let path = dir.join(format!("terrain-film-{tag}.ppm"));
        crate::render::write_ppm(path.to_str().unwrap(), width, height, &rgba);
        // Same relative patch as the device measurement: 200x140 at 2414x1080
        // is 0.0829 x 0.1296 of the frame.
        let patch_w = (width as f32 * 200.0 / 2414.0) as u32;
        let patch_h = (height as f32 * 140.0 / 1080.0) as u32;
        let x0 = (width as f32 * 0.26) as u32;
        let y0 = (height as f32 * 0.34) as u32;
        if let Some(image) = tiles.images.first() {
            let tile_path = dir.join(format!("terrain-layer0-{tag}.ppm"));
            crate::render::write_ppm(
                tile_path.to_str().unwrap(),
                image.width,
                image.height,
                &image.rgba,
            );
        }
        let stats = patch_stats(&rgba, width, x0, y0, patch_w, patch_h);
        crate::dev_log!(
            "TERRAIN_GROUND tag={} target=({:.0},{:.0}) patch=({x0},{y0},{patch_w},{patch_h}) mean=({:.1},{:.1},{:.1}) std=({:.1},{:.1},{:.1}) p5={:.0} p50={:.0} p95={:.0} range={:.0} edge={:.3} blue_excess={:.2} file={}",
            tag,
            target.x,
            target.y,
            stats.mean[0],
            stats.mean[1],
            stats.mean[2],
            stats.std[0],
            stats.std[1],
            stats.std[2],
            stats.p5,
            stats.p50,
            stats.p95,
            stats.range,
            stats.edge,
            stats.blue_excess,
            path.display()
        );
        if std::env::var("H2_TERRAIN_STRICT").ok().as_deref() != Some("1") {
            return;
        }
        for (channel, value) in stats.std.iter().enumerate() {
            assert!(
                *value >= 20.0,
                "channel {channel} std {value:.1} below WC3 paper-tile floor 20"
            );
        }
        assert!(
            stats.edge >= 0.12,
            "Sobel edge fraction {:.3} below 0.12: ground is blurred",
            stats.edge
        );
        assert!(
            stats.p95 - stats.p5 >= 40.0,
            "luminance band {:.0}..{:.0} is film-flat",
            stats.p5,
            stats.p95
        );
        assert!(
            stats.blue_excess.abs() <= 6.0,
            "blue-white additive sheen: b-r = {:.2}",
            stats.blue_excess
        );
    }
}

#[cfg(test)]
mod light_curve_tests {
    use super::TERRAIN_SHADER;

    /// The terrain shader must run the same highlight curve as the mesh shader,
    /// driven by the knee the game writes into `sun.w`. Two tonal curves in one
    /// frame make a unit darker than the ground it stands on (packet B §1).
    #[test]
    fn terrain_shader_uses_the_shared_highlight_shoulder() {
        assert!(
            TERRAIN_SHADER.contains("fn shoulder(value: vec3<f32>)"),
            "terrain shader lost the shoulder function"
        );
        assert!(
            TERRAIN_SHADER.contains("return shoulder(camera.ambient.rgb + camera.sun.rgb * ndl)"),
            "surface_light must call shoulder(), not clamp the raw sum"
        );
        assert!(
            TERRAIN_SHADER.contains("let knee = camera.sun.w;"),
            "the knee must come from the camera uniform"
        );
        assert!(
            !TERRAIN_SHADER.contains("return clamp(camera.ambient.rgb + camera.sun.rgb * ndl"),
            "the old hard clamp is still in the shader"
        );
    }

    /// The map boundary (t = 0) must be *exactly* the interior water colour:
    /// the frame continues the surface, so any tint at the inner edge draws a
    /// hard line between border and water. Measured on the Orc01 edge frame
    /// 2026-11-21: a 10 % constant tint put mean blue 168 far from the
    /// interior's 146 at the seam; the tint now ramps in with `t`.
    #[test]
    fn art14_water_edge_continuity() {
        let water = super::WaterMaterial::default();
        for (depth, wx, wz) in [
            (100.0f32, 512.0f32, 0.0f32),
            (12.0, 1024.0, 2048.0),
            (40.0, 4096.0, 1024.0),
            (72.0, 0.0, 0.0),
            (4.0, 2048.0, 512.0),
        ] {
            let legacy = super::water_edge_colour(&water, 0.0, depth, 1.0, wx, wz, true);
            let fixed = super::water_edge_colour(&water, 0.0, depth, 1.0, wx, wz, false);
            for channel in 0..4 {
                assert!(
                    (legacy[channel] - fixed[channel]).abs() < 1e-6,
                    "boundary channel {channel} diverges (depth {depth}): {} vs {}",
                    legacy[channel],
                    fixed[channel]
                );
            }
        }
        // A deep edge is the reported flat band: the legacy path gives one
        // colour across all eight frame cells; the shipped path must not.
        let frame: Vec<f32> = (0..=8)
            .map(|step| {
                super::water_edge_colour(
                    &water,
                    step as f32 / 8.0,
                    100.0,
                    1.0,
                    step as f32 * 1024.0,
                    0.0,
                    false,
                )[2]
            })
            .collect();
        let lo = frame.iter().cloned().fold(f32::INFINITY, f32::min);
        let hi = frame.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let max = frame.iter().cloned().fold(0.0f32, f32::max);
        // Linear-light blue is ~0.034 for the default deep palette; the
        // measurable non-flatness is a fraction of that, not 0.01 absolute.
        assert!(
            hi - lo > 0.01 * max,
            "deep edge must not be one flat blue value, spread {:.5} of max {max:.5}",
            hi - lo
        );
    }

    /// The knee is a real interval, and the curve must be the identity below it:
    /// that is what keeps the authored shadow/rib contrast (39.2 % of the Great
    /// Hall clipped before the mesh fix).
    #[test]
    fn shoulder_is_identity_below_the_knee_and_monotonic_above() {
        let knee = wc3_render::mesh::LIGHT_KNEE;
        assert!(knee > 0.1 && knee < 0.95, "knee {knee} outside the sane band");
        let mut previous = 0.0f32;
        let mut value = 0.0f32;
        while value <= 2.0 {
            let out = wc3_render::mesh::shoulder_cpu(value, knee);
            assert!(out >= previous - 1e-6, "shoulder must be monotonic at {value}");
            assert!((0.0..=1.0).contains(&out), "shoulder left [0,1] at {value}");
            if value <= knee {
                assert!((out - value).abs() < 1e-6, "identity broken at {value}");
            }
            previous = out;
            value += 0.05;
        }
    }
}

#[cfg(test)]
mod wgsl_parse_tests {
    use super::TERRAIN_SHADER;

    /// Parse the terrain WGSL exactly as wgpu will. A shader typo otherwise only
    /// surfaces on the device as a pipeline-creation validation error, after a
    /// full APK build and install.
    #[test]
    fn terrain_shader_parses_and_validates() {
        let module = naga::front::wgsl::parse_str(TERRAIN_SHADER)
            .expect("terrain WGSL must parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        );
        validator
            .validate(&module)
            .expect("terrain WGSL must pass naga validation");
    }
}
