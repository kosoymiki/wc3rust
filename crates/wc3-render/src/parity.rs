//! Merge/parity tooling: deterministic draw accounting and image comparison.
//!
//! The GPU-free half of the merge proof. `wc3-game/render.rs` (pre-merge) and
//! [`crate::Renderer`] (merged) must agree on three things for the same scene:
//!
//! 1. **Draw accounting** — [`account`] replays the visibility rules (geoset
//!    alpha, layer alpha, hidden helper planes, rings) and returns the exact
//!    number of draws and triangles the scene needs. After a frame, the
//!    renderer's [`Counters::draws`](crate::perf::Counters) must equal it.
//! 2. **Exact frame hash** — [`frame_hash`] hashes the RGBA8 readback. Two
//!    renderers that produce the same bytes return the same hash; the parity
//!    test asserts equality for the merged path versus the pre-merge path.
//! 3. **Perceptual distance** — [`mae`], [`max_diff`] and [`ssim`] quantify how
//!    far apart two frames are when they are not byte-identical (driver
//!    differences, animation phase).

use crate::mesh::{Model, PipelineKind};
use crate::scene::Scene;

/// What one frame of `scene` should cost, computed on the CPU.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SceneBudget {
    pub units: usize,
    pub geosets: usize,
    pub draws: usize,
    pub triangles: u64,
    pub vertices: u64,
    /// Draws per group: 0 = opaque/alpha-test, 1 = blended/additive, 2 = decals.
    pub groups: [usize; 3],
}

impl SceneBudget {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"units\":{},\"geosets\":{},\"draws\":{},\"triangles\":{},\"vertices\":{},\"groups\":[{},{},{}]}}",
            self.units,
            self.geosets,
            self.draws,
            self.triangles,
            self.vertices,
            self.groups[0],
            self.groups[1],
            self.groups[2]
        )
    }
}

/// Replay the renderer's visibility rules for `scene` and `models`.
///
/// Mirrors `MeshRenderer::prepare`: a geoset contributes one draw per visible
/// material layer; a geoset whose alpha is zero, whose indices are empty or that
/// is marked `hide` contributes nothing; each ring is one decal draw of 32
/// quads. `vertices` counts index references (vertex-shader invocations).
pub fn account(scene: &Scene, models: &[Model]) -> SceneBudget {
    let mut budget = SceneBudget {
        units: scene.units.len(),
        ..SceneBudget::default()
    };
    for unit in &scene.units {
        let Some(model) = models.get(unit.model) else {
            continue;
        };
        for (mesh_index, mesh) in model.iter().enumerate() {
            let geoset_alpha = unit.mesh_alpha.get(mesh_index).copied().unwrap_or(1.0);
            if geoset_alpha <= 0.001 || mesh.indices.is_empty() || mesh.positions.is_empty() {
                continue;
            }
            if mesh.hide {
                continue;
            }
            budget.geosets += 1;
            let mut visible = 0usize;
            for (layer_index, layer) in mesh.layers.iter().enumerate() {
                let layer_alpha = unit
                    .layer_alpha
                    .get(mesh_index)
                    .and_then(|alphas| alphas.get(layer_index))
                    .copied()
                    .unwrap_or(layer.alpha);
                if geoset_alpha * layer_alpha * unit.tint[3] <= 0.001 {
                    continue;
                }
                visible += 1;
                budget.draws += 1;
                budget.triangles += (mesh.indices.len() / 3) as u64;
                budget.vertices += mesh.indices.len() as u64;
                let group = PipelineKind::from_blend(layer.blend).group() as usize;
                budget.groups[group] += 1;
            }
            if visible == 0 {
                budget.geosets -= 1;
            }
        }
    }
    for _ring in &scene.rings {
        budget.draws += 1;
        budget.triangles += 32 * 2;
        budget.vertices += 32 * 6;
        budget.groups[2] += 1;
    }
    budget
}

/// FNV-1a 64 over the RGBA8 frame. Stable across runs and platforms.
pub fn frame_hash(rgba: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in rgba {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Mean absolute channel difference, excluding alpha. NaN on length mismatch.
pub fn mae(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::NAN;
    }
    let mut sum = 0u64;
    let mut count = 0u64;
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for channel in 0..3 {
            sum += (x[channel] as i32 - y[channel] as i32).unsigned_abs() as u64;
            count += 1;
        }
    }
    sum as f64 / count.max(1) as f64
}

/// Largest absolute channel difference (RGB), -1 on length mismatch.
pub fn max_diff(a: &[u8], b: &[u8]) -> i32 {
    if a.len() != b.len() || a.is_empty() {
        return -1;
    }
    let mut worst = 0i32;
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for channel in 0..3 {
            worst = worst.max((x[channel] as i32 - y[channel] as i32).abs());
        }
    }
    worst
}

/// Global SSIM on luma, computed over the whole frame.
///
/// `k1 = 0.01`, `k2 = 0.03`, `L = 255` (the standard constants). Returns 1.0
/// for identical images and NaN when the inputs do not match `width*height*4`.
pub fn ssim(a: &[u8], b: &[u8], width: u32, height: u32) -> f64 {
    let pixels = (width * height) as usize;
    if a.len() != pixels * 4 || b.len() != pixels * 4 || pixels == 0 {
        return f64::NAN;
    }
    let luma = |frame: &[u8], index: usize| -> f64 {
        let base = index * 4;
        // Rec. 709 luma.
        0.2126 * frame[base] as f64 + 0.7152 * frame[base + 1] as f64
            + 0.0722 * frame[base + 2] as f64
    };
    let n = pixels as f64;
    let mut sum_a = 0.0;
    let mut sum_b = 0.0;
    for index in 0..pixels {
        sum_a += luma(a, index);
        sum_b += luma(b, index);
    }
    let mean_a = sum_a / n;
    let mean_b = sum_b / n;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    let mut cov = 0.0;
    for index in 0..pixels {
        let da = luma(a, index) - mean_a;
        let db = luma(b, index) - mean_b;
        var_a += da * da;
        var_b += db * db;
        cov += da * db;
    }
    let (var_a, var_b, cov) = (var_a / n, var_b / n, cov / n);
    const C1: f64 = (0.01f64 * 255.0) * (0.01f64 * 255.0);
    const C2: f64 = (0.03f64 * 255.0) * (0.03f64 * 255.0);
    let numerator = (2.0 * mean_a * mean_b + C1) * (2.0 * cov + C2);
    let denominator = (mean_a * mean_a + mean_b * mean_b + C1) * (var_a + var_b + C2);
    if denominator == 0.0 {
        1.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{UiBatch, UnitDraw};
    use glam::{Mat4, Vec2, Vec3};

    fn unit() -> UnitDraw {
        UnitDraw {
            model: 0,
            world: Mat4::IDENTITY,
            tint: [1.0; 4],
            nodes: Vec::new(),
            mesh_alpha: vec![1.0],
            layer_alpha: vec![vec![1.0]],
            mesh_color: Vec::new(),
        }
    }

    fn scene(units: usize, rings: usize) -> Scene {
        Scene {
            view_proj: Mat4::IDENTITY,
            eye: Vec3::ZERO,
            viewport: Vec2::new(64.0, 64.0),
            units: (0..units).map(|_| unit()).collect(),
            rings: (0..rings)
                .map(|index| crate::scene::Ring {
                    pos: Vec3::new(index as f32, 0.0, 0.0),
                    radius: 4.0,
                    color: [1.0; 4],
                })
                .collect(),
            ui: UiBatch::default(),
        }
    }

    fn model(layers: usize, hide: bool) -> Model {
        vec![crate::mesh::MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Z],
            normals: vec![Vec3::Y; 3],
            uvs: vec![Vec2::ZERO; 3],
            uvs2: Vec::new(),
            indices: vec![0, 1, 2],
            bones: vec![Vec::new(); 3],
            layers: (0..layers)
                .map(|index| crate::mesh::MeshLayer {
                    texture: index,
                    alpha: 1.0,
                    blend: index as u32,
                    team_color: index % 2 == 0,
                    uv_set: 0,
                })
                .collect(),
            z_up: false,
            hide,
        }]
    }

    #[test]
    fn accounting_counts_one_draw_per_visible_layer() {
        let models = vec![model(3, false)];
        let budget = account(&scene(2, 1), &models);
        assert_eq!(budget.units, 2);
        assert_eq!(budget.geosets, 2);
        assert_eq!(budget.draws, 2 * 3 + 1, "three layers per unit plus one ring");
        assert_eq!(budget.triangles, 2 * 3 + 64);
        // Layer blends are 0 (opaque), 1 (cutout), 2 (blend): opaque and cutout
        // share group 0, blend goes to group 1, the ring is a decal in group 2.
        assert_eq!(budget.groups[0], 4, "opaque + cutout per unit");
        assert_eq!(budget.groups[1], 2, "blend layer per unit");
        assert_eq!(budget.groups[2], 1, "the ring is a decal");
    }

    #[test]
    fn hidden_geosets_and_zero_alpha_are_free() {
        let models = vec![model(2, true)];
        let budget = account(&scene(3, 0), &models);
        assert_eq!(budget.draws, 0);
        let mut scene = scene(1, 0);
        scene.units[0].mesh_alpha = vec![0.0];
        let budget = account(&scene, &vec![model(2, false)]);
        assert_eq!(budget.draws, 0);
        assert_eq!(budget.geosets, 0);
    }

    #[test]
    fn frame_hash_is_stable_and_sensitive() {
        let a = vec![0u8, 1, 2, 3, 4, 5, 6, 7];
        let b = a.clone();
        assert_eq!(frame_hash(&a), frame_hash(&b));
        let mut c = a.clone();
        c[7] = 8;
        assert_ne!(frame_hash(&a), frame_hash(&c));
    }

    #[test]
    fn metrics_behave() {
        let a = vec![10u8; 16 * 4];
        let b = vec![10u8; 16 * 4];
        assert_eq!(mae(&a, &b), 0.0);
        assert_eq!(max_diff(&a, &b), 0);
        // 16 pixels of RGBA8 = 4x4.
        assert!((ssim(&a, &b, 4, 4) - 1.0).abs() < 1e-9);
        let mut c = b.clone();
        c[0] = 40;
        assert!(mae(&a, &c) > 0.0);
        assert_eq!(max_diff(&a, &c), 30);
        assert!(ssim(&a, &c, 4, 4) < 1.0);
    }
}
