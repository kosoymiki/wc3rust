//! Terrain overlay mechanics: the map-edge water border + shoreline band
//! (`render.terrain.water-shoreline`) and the cliff plateau/face
//! (`render.terrain.cliff-face`).
//!
//! These exercise the pure builders in `wc3_render::terrain_geometry`; the
//! rendered proof lives in `wc3-game`'s Justice close-ups because the GPU
//! pipeline is game-layer. Units are world units (128 per W3E tile).

use wc3_render::terrain_geometry::{cliff_blocks, shoreline, water_border, water_depth_color, Corner, Grid};

const TILE: f32 = 128.0;
const LAND: f32 = 20.0;
const WATER: f32 = 5.0;

fn land() -> Corner {
    Corner {
        height: LAND,
        water: WATER,
        layer: 2,
        water_flag: false,
        ramp: false,
        cliff_variation: 0,
        cliff_texture: 0,
    }
}

/// `size x size` corners with the `2..=4` square marked wet.
fn pond_grid(size: usize) -> Grid {
    let mut corners = vec![land(); size * size];
    for z in 2..=4 {
        for x in 2..=4 {
            corners[z * size + x].water_flag = true;
        }
    }
    Grid::new(size, size, corners)
}

/// Same pond, plus two wet corners on the left map boundary so a border strip
/// must exist on `x = 0`.
fn edge_pond_grid(size: usize) -> Grid {
    let mut grid = pond_grid(size);
    for z in 3..=4 {
        grid.corners[z * size].water_flag = true;
    }
    grid
}

fn quad_count(mesh: &wc3_render::terrain_geometry::Mesh) -> usize {
    mesh.indices.len() / 6
}

/// `render.terrain.water-shoreline`: Water.slk depth-ramp alpha (shore fade),
/// a foam quad on every wet/dry shore segment, and the map-edge water border
/// frame that replaces the old flat clear-colour band.
#[test]
fn render_terrain_water_shoreline() {
    // 1. Water.slk ramp: alpha 10/255 at the shoreline, 250/255 in deep water.
    let shallow_min = [1.0, 1.0, 1.0, 10.0 / 255.0];
    let shallow_max = [0.46, 0.46, 0.78, 219.0 / 255.0];
    let deep_min = [0.46, 0.46, 0.78, 219.0 / 255.0];
    let deep_max = [0.38, 0.38, 0.75, 250.0 / 255.0];
    let shore = water_depth_color(
        shallow_min,
        shallow_max,
        deep_min,
        deep_max,
        0.0,
        10.0,
        64.0,
        72.0,
    );
    assert!(
        (shore[3] - 10.0 / 255.0).abs() < 1e-6,
        "dry shore corner takes Smin alpha, got {}",
        shore[3]
    );
    let deep = water_depth_color(
        shallow_min,
        shallow_max,
        deep_min,
        deep_max,
        200.0,
        10.0,
        64.0,
        72.0,
    );
    assert!(
        (deep[3] - 250.0 / 255.0).abs() < 1e-6,
        "deep water takes Dmax alpha, got {}",
        deep[3]
    );
    assert!(deep[0] < shore[0], "deep water is darker than the shore");

    // 2. Foam band: one quad per wet/dry lattice edge. A 3x3 wet corner square
    //    has 12 boundary edges (4 per side), each quadrant edge exactly once.
    let grid = pond_grid(7);
    let foam = shoreline(&grid, 16.0, TILE, 1.5, 256.0);
    assert_eq!(quad_count(&foam), 12, "one foam quad per shore segment");
    for vertex in &foam.vertices {
        assert!(
            (vertex.position[1] - (WATER + 1.5)).abs() < 1e-4,
            "foam sits on the water surface, got {}",
            vertex.position[1]
        );
        assert!(vertex.uv[1].abs() < 1e-6 || (vertex.uv[1] - 1.0).abs() < 1e-6);
    }
    // The band straddles the shoreline: its centre is a lattice edge.
    let mut centres: Vec<f32> = foam
        .vertices
        .chunks_exact(4)
        .map(|quad| {
            let x = (quad[0].position[0] + quad[1].position[0]) * 0.5;
            let z = (quad[0].position[2] + quad[1].position[2]) * 0.5;
            if (quad[0].position[0] - quad[1].position[0]).abs() < 1e-6 {
                z
            } else {
                x
            }
        })
        .collect();
    centres.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for centre in centres {
        let on_lattice = (centre / TILE - (centre / TILE).round()).abs() < 1e-3;
        assert!(on_lattice, "shore band centre {centre} is on a lattice edge");
    }

    // 3. Map-edge border: the left side is wet, the right side is dry, so the
    //    frame exists on `x = 0` only and fades out at its outer row.
    let grid = edge_pond_grid(7);
    let deep = [0.10, 0.10, 0.30, 0.98];
    let border = water_border(&grid, 4, 2, TILE, deep);
    assert!(!border.is_empty(), "wet map edge gets a water border frame");
    let outer = 6.0 * TILE;
    for vertex in &border.vertices {
        let [x, _, z] = vertex.position;
        let outside = x < -1e-3 || x > outer + 1e-3 || z < -1e-3 || z > outer + 1e-3;
        let on_boundary = (x.abs() < 1e-3 || (x - outer).abs() < 1e-3)
            && z >= -1e-3
            && z <= outer + 1e-3;
        let on_boundary = on_boundary
            || ((z.abs() < 1e-3 || (z - outer).abs() < 1e-3) && x >= -1e-3 && x <= outer + 1e-3);
        assert!(
            outside || on_boundary,
            "border vertex ({x},{z}) must be outside the map or on its edge"
        );
        assert!(x <= 1e-3, "the dry right edge must not be framed");
    }
    // Interior rows are fully opaque deep water; the outermost row is faded.
    let mut rows: Vec<(f32, f32)> = border
        .vertices
        .iter()
        .filter(|v| v.position[0] < -1.0)
        .map(|v| (v.tag[0], v.color[3]))
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let first = rows.first().copied().expect("border rows");
    let last = rows.last().copied().expect("border rows");
    assert!(
        first.1 > last.1,
        "border alpha must fade outward: {} -> {}",
        first.1,
        last.1
    );
    assert!(last.1.abs() < 1e-3, "outer border row fades to zero alpha");
    assert!(
        (rows.iter().map(|r| r.1).fold(0.0f32, f32::max) - deep[3]).abs() < 1e-3,
        "interior border rows carry the authored deep alpha"
    );
    // The wet boundary row sits at the corner's water surface, so the frame
    // meets the interior water mesh without a step.
    let on_edge = border
        .vertices
        .iter()
        .filter(|v| v.position[0].abs() < 1e-3)
        .all(|v| (v.position[1] - WATER).abs() < 1e-4);
    assert!(on_edge, "border starts exactly at the W3E water level");
}

/// `render.terrain.cliff-face`: a TFT layer transition becomes a flat plateau
/// top with a vertical rock face instead of a smooth 45° ramp, and the face
/// carries the map's cliff texture/variation for the shader.
#[test]
fn render_terrain_cliff_face() {
    const SIZE: usize = 8;
    const HIGH: f32 = 512.0;
    let mut corners = vec![land(); SIZE * SIZE];
    for z in 3..=4 {
        for x in 3..=4 {
            corners[z * SIZE + x].height = HIGH;
            corners[z * SIZE + x].layer = 6;
            corners[z * SIZE + x].cliff_variation = 5;
            corners[z * SIZE + x].cliff_texture = 1;
        }
    }
    let grid = Grid::new(SIZE, SIZE, corners);
    let lift = 1.0;
    let blocks = cliff_blocks(&grid, TILE, lift);
    assert!(!blocks.is_empty(), "layer transition emits a cliff block");

    let tops: Vec<_> = blocks
        .vertices
        .iter()
        .filter(|v| v.tag[1] > 0.5)
        .collect();
    let walls: Vec<_> = blocks
        .vertices
        .iter()
        .filter(|v| v.tag[1] < 0.5)
        .collect();
    assert!(!tops.is_empty(), "cliff plateau has a flat top");
    assert!(!walls.is_empty(), "cliff plateau has vertical faces");
    assert_eq!(tops.len() % 4, 0, "tops are quads");
    assert_eq!(walls.len() % 4, 0, "faces are quads");
    for vertex in &tops {
        assert!(
            (vertex.position[1] - (HIGH + lift)).abs() < 1e-4,
            "plateau top is flat at the upper layer, got {}",
            vertex.position[1]
        );
        assert!(
            (0.0..4.0).contains(&vertex.tag[3]),
            "top carries its source corner for the ground material"
        );
        assert_eq!(vertex.tag[2] as u8, 1, "top carries the map cliff texture");
    }
    // Every wall hangs from a plateau top down to a real surface (0 or another
    // plateau top) and is vertical: the two top vertices share Y, the two
    // bottom vertices share Y.
    for quad in walls.chunks_exact(4) {
        let top_y = (quad[0].position[1] + quad[1].position[1]) * 0.5;
        let bottom_y = (quad[2].position[1] + quad[3].position[1]) * 0.5;
        assert!((quad[0].position[1] - quad[1].position[1]).abs() < 1e-3);
        assert!((quad[2].position[1] - quad[3].position[1]).abs() < 1e-3);
        assert!(
            top_y > bottom_y + 1.0,
            "wall drops {:.1} units to {:.1}",
            top_y,
            bottom_y
        );
        assert!(
            (top_y - (HIGH + lift)).abs() < 1e-3 || top_y < HIGH + lift,
            "wall top is a plateau"
        );
        assert!(
            (bottom_y - LAND).abs() < 1e-3
                || bottom_y.abs() < 1e-3
                || (bottom_y - (HIGH + lift)).abs() < 1e-3,
            "wall bottom is the lower ground or a lower plateau, got {bottom_y}"
        );
        // Vertical face: bottom and top share the same horizontal position.
        for (top, bottom) in [(quad[0], quad[3]), (quad[1], quad[2])] {
            assert!((top.position[0] - bottom.position[0]).abs() < 1e-3);
            assert!((top.position[2] - bottom.position[2]).abs() < 1e-3);
        }
        // UV: v = 0 at the top lip and > 0 at the bottom; u advances along the
        // edge so the cliff sheet does not stretch.
        assert!(quad[0].uv[1].abs() < 1e-6 && quad[1].uv[1].abs() < 1e-6);
        assert!(quad[2].uv[1] > 0.9 && quad[3].uv[1] > 0.9);
        assert!((quad[0].uv[0] - quad[1].uv[0]).abs() > 0.99);
    }
    // A fully submerged cell with a layer spread keeps the smooth sea floor.
    let mut wet = vec![land(); 16];
    for corner in &mut wet {
        corner.water_flag = true;
    }
    wet[5].height = HIGH;
    wet[5].layer = 6;
    assert!(
        cliff_blocks(&Grid::new(4, 4, wet), TILE, lift).is_empty(),
        "no cliff block inside open water"
    );
    // A flat grid (no layer change) and a ramp transition emit nothing.
    let flat = Grid::new(4, 4, vec![land(); 16]);
    assert!(cliff_blocks(&flat, TILE, lift).is_empty());
    let mut ramp = vec![land(); 16];
    for corner in &mut ramp {
        corner.ramp = true;
    }
    ramp[5].height = HIGH;
    ramp[5].layer = 6;
    let ramp_grid = Grid::new(4, 4, ramp);
    assert!(
        cliff_blocks(&ramp_grid, TILE, lift).is_empty(),
        "ramps stay smooth; no cliff block"
    );
    // The atlas quadrant selector: variation 5 -> column 1, row 1.
    assert_eq!(
        wc3_render::terrain_geometry::sheet_quadrant(5),
        [0.25, 0.25]
    );
}
