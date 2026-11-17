//! Terrain overlay geometry: map-edge water border, shoreline foam band and
//! cliff blocks.
//!
//! The W3E grid is a `(cells + 1)²` corner lattice. The game layer converts it
//! into [`Grid`] (heights already in world units) and calls the three builders
//! here. They are pure geometry: no GPU, no archive access, deterministic, so
//! the water/shoreline/cliff contracts are testable without a device.
//!
//! * [`water_border`] extrudes the water surface past the map boundary wherever
//!   the boundary corner is wet. WC3 clamps the camera to the map, so the
//!   original client never shows what lies beyond the last tile; the mobile
//!   build can zoom out, and the pre-fix renderer showed the engine clear
//!   colour there — the operator's "flat blue band at the map edge". The border
//!   is a deep-water frame that fades out over its last rows.
//! * [`shoreline`] places a foam band on every corner pair whose W3E water flag
//!   differs. Water.slk names the authored shore models (`Shoreline`,
//!   `ShorelineWave`, …); this is the same one-quad-per-shore-segment contract
//!   the `ShoreWaveModel` class implements in Game.dll.
//! * [`cliff_blocks`] replaces the smooth 45° interpolation across a TFT cliff
//!   layer transition with a flat plateau top plus vertical faces textured from
//!   the map's `Cliff*.blp` sheet. The engine places authored
//!   `Cliffs<letters><variation>.mdx` models there; the block is the same
//!   footprint without loading model geometry.
//!
//! Units: positions and `tile` are world units (128 per W3E tile), heights are
//! final W3E heights (corner height + layer − 2).

/// One W3E corner as the overlay builders need it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    /// Final ground height in world units.
    pub height: f32,
    /// Final water surface height in world units.
    pub water: f32,
    /// TFT cliff layer (`cliff_data & 0x0F`, 2 = sea level).
    pub layer: u8,
    /// W3E water flag (`texture_flags & 0x40`).
    pub water_flag: bool,
    /// W3E ramp flag (`texture_flags & 0x10`): ramps stay smooth.
    pub ramp: bool,
    /// W3E cliff variation (`variation >> 5`, three bits) — picks the cliff
    /// sheet quadrant on a face.
    pub cliff_variation: u8,
    /// W3E per-corner cliff texture index (`cliff_data >> 4`) — picks the
    /// map's `Cliff0`/`Cliff1` array layer.
    pub cliff_texture: u8,
}

impl Default for Corner {
    fn default() -> Self {
        Self {
            height: 0.0,
            water: 0.0,
            layer: 2,
            water_flag: false,
            ramp: false,
            cliff_variation: 0,
            cliff_texture: 0,
        }
    }
}

/// Corner lattice: `width * height` corners, `(width - 1) * (height - 1)` cells.
#[derive(Clone, Debug)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub corners: Vec<Corner>,
}

impl Grid {
    pub fn new(width: usize, height: usize, corners: Vec<Corner>) -> Self {
        assert_eq!(
            corners.len(),
            width * height,
            "corner count must be width*height"
        );
        Self {
            width,
            height,
            corners,
        }
    }

    pub fn at(&self, x: usize, z: usize) -> Option<&Corner> {
        (x < self.width && z < self.height).then(|| &self.corners[z * self.width + x])
    }

    pub fn cells_w(&self) -> usize {
        self.width.saturating_sub(1)
    }

    pub fn cells_h(&self) -> usize {
        self.height.saturating_sub(1)
    }
}

/// One overlay vertex. `tag` is pass-specific:
///
/// * water border — `[fade 0..1, 1, 0, 0]`;
/// * shoreline — `[0, 0, 0, 0]`;
/// * cliff — `[cliff_variation, is_top (1) / wall (0), cliff_texture, source corner]`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub tag: [f32; 4],
    /// Grid cell `(x, z)` the vertex belongs to (cliff tops/walls need it to
    /// copy the cell's ground material).
    pub cell: [f32; 2],
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    fn quad(&mut self, a: Vertex, b: Vertex, c: Vertex, d: Vertex) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[a, b, c, d]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Number of atlas quadrants per sheet axis (the 4x4 pack).
const SHEET_GRID: u32 = 4;

/// A cliff face shorter than this is the 0.2-tile lip between a cliff cell and
/// an already-high neighbour, not a visible rock face.
const MIN_WALL: f32 = 8.0;

/// Extrude the water surface past the map boundary.
///
/// Every boundary segment whose two corners are not both dry gets a strip of
/// `border_cells` quads outward, at the wet corner's own surface height, so the
/// sea continues past the last playable tile instead of ending on the engine
/// clear colour. The outermost `fade_cells` rows fade alpha to zero, which lets
/// the clear colour read as distance haze instead of a hard band edge. UVs are
/// world-space, continuous with the interior water mesh.
pub fn water_border(
    grid: &Grid,
    border_cells: u32,
    fade_cells: u32,
    tile: f32,
    deep: [f32; 4],
) -> Mesh {
    let mut mesh = Mesh::default();
    let border_cells = border_cells.max(1);
    let fade = fade_cells.min(border_cells);
    let fade_start = 1.0 - fade as f32 / border_cells as f32;
    let alpha_at = |t: f32| -> f32 {
        if fade == 0 || t <= fade_start + 1e-6 {
            1.0
        } else {
            ((1.0 - t) / (1.0 - fade_start).max(1e-6)).clamp(0.0, 1.0)
        }
    };
    let vertex = |p: [f32; 3], t: f32| Vertex {
        position: p,
        uv: [p[0], p[2]],
        color: [deep[0], deep[1], deep[2], deep[3] * alpha_at(t)],
        tag: [t, 1.0, 0.0, 0.0],
        cell: [0.0, 0.0],
    };
    let sides: [(f32, f32, f32, f32); 4] = [
        (-1.0, 0.0, 0.0, 1.0),
        (1.0, 0.0, 0.0, 1.0),
        (0.0, -1.0, 1.0, 0.0),
        (0.0, 1.0, 1.0, 0.0),
    ];
    for (dir_x, dir_z, along_x, along_z) in sides {
        let segments = if along_x != 0.0 {
            grid.cells_w()
        } else {
            grid.cells_h()
        };
        for segment in 0..segments {
            let (sx, sz) = if dir_x < 0.0 {
                (0.0, segment as f32 * tile)
            } else if dir_x > 0.0 {
                ((grid.width - 1) as f32 * tile, segment as f32 * tile)
            } else if dir_z < 0.0 {
                (segment as f32 * tile, 0.0)
            } else {
                (segment as f32 * tile, (grid.height - 1) as f32 * tile)
            };
            let ex = sx + along_x * tile;
            let ez = sz + along_z * tile;
            let corner_at = |x: f32, z: f32| -> Corner {
                let xi = (x / tile).round().clamp(0.0, (grid.width - 1) as f32) as usize;
                let zi = (z / tile).round().clamp(0.0, (grid.height - 1) as f32) as usize;
                *grid.at(xi, zi).unwrap_or(&Corner::default())
            };
            let a = corner_at(sx, sz);
            let b = corner_at(ex, ez);
            if !a.water_flag && !b.water_flag {
                continue;
            }
            // Per-corner surface heights: the interior water mesh is built from
            // each corner's own W3E water level, so the frame must continue that
            // lattice exactly. Substituting the wet neighbour's level at a dry
            // corner lifts the frame off the interior surface (a visible step).
            let la = a.water;
            let lb = b.water;
            for row in 0..border_cells {
                let t0 = row as f32 / border_cells as f32;
                let t1 = (row + 1) as f32 / border_cells as f32;
                let x0 = sx + dir_x * row as f32 * tile;
                let z0 = sz + dir_z * row as f32 * tile;
                let x1 = sx + dir_x * (row + 1) as f32 * tile;
                let z1 = sz + dir_z * (row + 1) as f32 * tile;
                mesh.quad(
                    vertex([x0, la, z0], t0),
                    vertex([x0 + along_x * tile, lb, z0 + along_z * tile], t0),
                    vertex([x1 + along_x * tile, lb, z1 + along_z * tile], t1),
                    vertex([x1, la, z1], t1),
                );
            }
        }
    }
    mesh
}

/// Foam band on every shore segment.
///
/// Iterates the corner lattice once and places one quad wherever two adjacent
/// corners disagree on the W3E water flag: the shoreline passes between them,
/// so the band is centred on that lattice edge at the wet corner's surface
/// height plus `lift` world units (avoids z-fighting with the water quad).
/// UVs advance one repeat per `uv_period` world units so the authored
/// `Shoreline1.blp` wave strip tiles along the shore.
pub fn shoreline(grid: &Grid, half_width: f32, tile: f32, lift: f32, uv_period: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let period = uv_period.max(1.0);
    let mut band = |ax: f32, az: f32, bx: f32, bz: f32, a: &Corner, b: &Corner| {
        if a.water_flag == b.water_flag {
            return;
        }
        let level = if a.water_flag { a.water } else { b.water } + lift;
        let horizontal = (bz - az).abs() < 1e-3;
        let (nx, nz) = if horizontal {
            (0.0, half_width)
        } else {
            (half_width, 0.0)
        };
        let u0 = if horizontal { ax } else { az } / period;
        let u1 = if horizontal { bx } else { bz } / period;
        let vertex = |p: [f32; 3], u: f32, v: f32| Vertex {
            position: p,
            uv: [u, v],
            color: [1.0, 1.0, 1.0, 1.0],
            tag: [0.0, 0.0, 0.0, 0.0],
            cell: [0.0, 0.0],
        };
        mesh.quad(
            vertex([ax - nx, level, az - nz], u0, 0.0),
            vertex([ax + nx, level, az + nz], u0, 1.0),
            vertex([bx + nx, level, bz + nz], u1, 1.0),
            vertex([bx - nx, level, bz - nz], u1, 0.0),
        );
    };
    for z in 0..grid.height {
        for x in 0..grid.cells_w() {
            let (Some(a), Some(b)) = (grid.at(x, z), grid.at(x + 1, z)) else {
                continue;
            };
            band(
                x as f32 * tile,
                z as f32 * tile,
                (x + 1) as f32 * tile,
                z as f32 * tile,
                a,
                b,
            );
        }
    }
    for z in 0..grid.cells_h() {
        for x in 0..grid.width {
            let (Some(a), Some(b)) = (grid.at(x, z), grid.at(x, z + 1)) else {
                continue;
            };
            band(
                x as f32 * tile,
                z as f32 * tile,
                x as f32 * tile,
                (z + 1) as f32 * tile,
                a,
                b,
            );
        }
    }
    mesh
}

/// Flat cliff plateau tops plus vertical faces.
///
/// A cell whose four corner layers differ is a cliff transition: the heightfield
/// interpolation over it is a 45° ramp, while the engine places an authored
/// `Cliffs<A..D><variation>.mdx` model (a flat top at the upper layer and a
/// vertical rock face). This builder emits the same footprint without the model:
///
/// * a flat top quad at `max(corner heights) + lift` for every cliff cell;
/// * a vertical face on each cell edge where the neighbouring surface is lower,
///   spanning from the plateau top down to that surface.
///
/// `tag` on top vertices carries the source corner index in `.w` so the caller
/// can copy the ground material (weights/colour) of the highest corner; wall
/// vertices carry `[variation, 0, cliff_texture, 0]`. Wall UVs are
/// `(along / tile, (top - y) / tile)`, so one cliff-sheet quadrant is sampled
/// per wall vertex.
pub fn cliff_blocks(grid: &Grid, tile: f32, lift: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let cw = grid.cells_w();
    let ch = grid.cells_h();
    if cw == 0 || ch == 0 {
        return mesh;
    }
    let mut top: Vec<Option<f32>> = vec![None; cw * ch];
    let mut highest: Vec<u8> = vec![0; cw * ch];
    for z in 0..ch {
        for x in 0..cw {
            let quad = cell_corners(grid, x, z);
            let min_layer = quad.iter().map(|c| c.layer).min().unwrap_or(0);
            let max_layer = quad.iter().map(|c| c.layer).max().unwrap_or(0);
            // Fully submerged cells keep the smooth sea floor: a rock plateau in
            // open water is never what the W3E author drew.
            if min_layer == max_layer
                || quad.iter().any(|c| c.ramp)
                || quad.iter().all(|c| c.water_flag)
            {
                continue;
            }
            let mut best = 0usize;
            for (index, corner) in quad.iter().enumerate() {
                if corner.height > quad[best].height {
                    best = index;
                }
            }
            highest[z * cw + x] = best as u8;
            top[z * cw + x] = Some(quad[best].height + lift);
        }
    }
    let top_height = |x: usize, z: usize| -> Option<f32> {
        if x < cw && z < ch {
            top[z * cw + x]
        } else {
            None
        }
    };
    for z in 0..ch {
        for x in 0..cw {
            let Some(height) = top[z * cw + x] else {
                continue;
            };
            let quad = cell_corners(grid, x, z);
            let source = &quad[highest[z * cw + x] as usize];
            let vertex = |px: f32, pz: f32, corner: usize| Vertex {
                position: [
                    x as f32 * tile + px * tile,
                    height,
                    z as f32 * tile + pz * tile,
                ],
                uv: [x as f32 + px, z as f32 + pz],
                color: [1.0, 1.0, 1.0, 1.0],
                tag: [
                    source.cliff_variation as f32,
                    1.0,
                    source.cliff_texture as f32,
                    corner as f32,
                ],
                cell: [x as f32, z as f32],
            };
            mesh.quad(
                vertex(0.0, 0.0, 0),
                vertex(1.0, 0.0, 1),
                vertex(1.0, 1.0, 2),
                vertex(0.0, 1.0, 3),
            );
        }
    }
    // One wall per shared edge, emitted by the higher cell only.
    for z in 0..ch {
        for x in 0..cw {
            let Some(my_top) = top[z * cw + x] else {
                continue;
            };
            let quad = cell_corners(grid, x, z);
            let source = &quad[highest[z * cw + x] as usize];
            let wall = |mesh: &mut Mesh,
                        ax: f32,
                        az: f32,
                        bx: f32,
                        bz: f32,
                        bottom_a: f32,
                        bottom_b: f32| {
                let horizontal = (bz - az).abs() < 1e-3;
                let u0 = if horizontal { ax } else { az } / tile;
                let u1 = if horizontal { bx } else { bz } / tile;
                let vertex = |p: [f32; 3], u: f32, v: f32| Vertex {
                    position: p,
                    uv: [u, v],
                    color: [1.0, 1.0, 1.0, 1.0],
                    tag: [
                        source.cliff_variation as f32,
                        0.0,
                        source.cliff_texture as f32,
                        0.0,
                    ],
                    cell: [x as f32, z as f32],
                };
                mesh.quad(
                    vertex([ax, my_top, az], u0, 0.0),
                    vertex([bx, my_top, bz], u1, 0.0),
                    vertex([bx, bottom_b, bz], u1, (my_top - bottom_b).max(0.05) / tile),
                    vertex([ax, bottom_a, az], u0, (my_top - bottom_a).max(0.05) / tile),
                );
            };
            // Bottom of a wall: the neighbour's plateau top if it is a cliff
            // cell, otherwise the shared corner heights (the neighbour's own
            // surface along that edge).
            // Left edge (x fixed): shared with cell (x-1, z).
            let a = quad[0];
            let b = quad[3];
            let neighbour = if x > 0 { top_height(x - 1, z) } else { None };
            let (ba, bb) = match neighbour {
                Some(other) => (other, other),
                None if x > 0 => (
                    grid.at(x, z).map(|c| c.height).unwrap_or(a.height),
                    grid.at(x, z + 1).map(|c| c.height).unwrap_or(b.height),
                ),
                None => (a.height, b.height),
            };
            if my_top > ba.max(bb) + MIN_WALL {
                wall(
                    &mut mesh,
                    x as f32 * tile,
                    z as f32 * tile,
                    x as f32 * tile,
                    (z + 1) as f32 * tile,
                    ba,
                    bb,
                );
            }
            // Bottom edge (z fixed): shared with cell (x, z-1).
            let a = quad[0];
            let b = quad[1];
            let neighbour = if z > 0 { top_height(x, z - 1) } else { None };
            let (ba, bb) = match neighbour {
                Some(other) => (other, other),
                None if z > 0 => (
                    grid.at(x, z).map(|c| c.height).unwrap_or(a.height),
                    grid.at(x + 1, z).map(|c| c.height).unwrap_or(b.height),
                ),
                None => (a.height, b.height),
            };
            if my_top > ba.max(bb) + MIN_WALL {
                wall(
                    &mut mesh,
                    x as f32 * tile,
                    z as f32 * tile,
                    (x + 1) as f32 * tile,
                    z as f32 * tile,
                    ba,
                    bb,
                );
            }
            // Right edge (x+1 fixed): shared with cell (x+1, z). Emit only if
            // the neighbour is lower AND is not a cliff cell (a cliff neighbour
            // emits its own left wall).
            if x + 1 < cw {
                let neighbour = top_height(x + 1, z);
                let a = quad[1];
                let b = quad[2];
                let (ba, bb) = match neighbour {
                    Some(other) => (other, other),
                    None => (
                        grid.at(x + 1, z).map(|c| c.height).unwrap_or(a.height),
                        grid.at(x + 1, z + 1).map(|c| c.height).unwrap_or(b.height),
                    ),
                };
                if my_top > ba.max(bb) + MIN_WALL {
                    wall(
                        &mut mesh,
                        (x + 1) as f32 * tile,
                        z as f32 * tile,
                        (x + 1) as f32 * tile,
                        (z + 1) as f32 * tile,
                        ba,
                        bb,
                    );
                }
            }
            // Top edge (z+1 fixed): shared with cell (x, z+1).
            if z + 1 < ch {
                let neighbour = top_height(x, z + 1);
                let a = quad[3];
                let b = quad[2];
                let (ba, bb) = match neighbour {
                    Some(other) => (other, other),
                    None => (
                        grid.at(x, z + 1).map(|c| c.height).unwrap_or(a.height),
                        grid.at(x + 1, z + 1).map(|c| c.height).unwrap_or(b.height),
                    ),
                };
                if my_top > ba.max(bb) + MIN_WALL {
                    wall(
                        &mut mesh,
                        x as f32 * tile,
                        (z + 1) as f32 * tile,
                        (x + 1) as f32 * tile,
                        (z + 1) as f32 * tile,
                        ba,
                        bb,
                    );
                }
            }
        }
    }
    mesh
}

fn cell_corners(grid: &Grid, x: usize, z: usize) -> [Corner; 4] {
    [
        grid.at(x, z).copied().unwrap_or_default(),
        grid.at(x + 1, z).copied().unwrap_or_default(),
        grid.at(x + 1, z + 1).copied().unwrap_or_default(),
        grid.at(x, z + 1).copied().unwrap_or_default(),
    ]
}

/// WC3 depth ramp (HiveWE `data/shaders/water.vert`): interpolate the authored
/// `Smin -> Smax` (0..`deep_level`) and `Dmin -> Dmax` (`deep_level`..`max_depth`)
/// RGBA rows by water depth in world units. Alpha is part of the authored row,
/// which is what fades the surface out at the shore.
pub fn water_depth_color(
    shallow_min: [f32; 4],
    shallow_max: [f32; 4],
    deep_min: [f32; 4],
    deep_max: [f32; 4],
    depth: f32,
    min_depth: f32,
    deep_level: f32,
    max_depth: f32,
) -> [f32; 4] {
    let mix4 = |a: [f32; 4], b: [f32; 4], t: f32| -> [f32; 4] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]
    };
    if depth <= deep_level {
        let t = ((depth - min_depth) / (deep_level - min_depth).max(1e-3)).clamp(0.0, 1.0);
        mix4(shallow_min, shallow_max, t)
    } else {
        let t = ((depth - deep_level) / (max_depth - deep_level).max(1e-3)).clamp(0.0, 1.0);
        mix4(deep_min, deep_max, t)
    }
}

/// Cliff sheet quadrant for a W3E cliff variation (three bits, 0..7) in a 4x4
/// atlas: `(column, row) / 4` in UV units. Mirrors the ground variation rule
/// (`variant % 4`, `variant / 4`) so both sheets use one quadrant selection.
pub fn sheet_quadrant(variation: u8) -> [f32; 2] {
    let slot = (variation % SHEET_GRID as u8) as f32;
    let row = (variation / SHEET_GRID as u8) as f32;
    [slot / SHEET_GRID as f32, row / SHEET_GRID as f32]
}
