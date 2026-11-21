//! Agent A art push — terrain shoulder + map-edge water (art14).
//!
//! Deterministic CPU tests. `art14_terrain_shoulder` proves the sun-facing
//! slope no longer clips flat and that the terrain shader actually calls the
//! same curve as the mesh shader. `art14_water_edge` proves the old edge
//! colour was one flat value and the shipped path produces a non-flat,
//! palette-bounded shallow→deep transition.

use wc3_game::terrain_render::{water_edge_colour, TERRAIN_SHADER, WaterMaterial};
use wc3_render::mesh::{shoulder_cpu, LIGHT_KNEE};

#[test]
fn art14_terrain_shoulder() {
    // Raw terrain DNC lights from PATCH-REQUESTS.md: sun 1.3, ambient 0.2.
    // A hard clamp passes 1.0 at N.L = 0.615, so every sunnier slope is white.
    let (sun, ambient) = (1.3f32, 0.2f32);
    let raw = |ndl: f32| ambient + sun * ndl;
    let clip = |v: f32| v.clamp(0.0, 1.0);
    let shoulder = |v: f32| shoulder_cpu(v, LIGHT_KNEE);

    assert!(
        (clip(raw(0.615)) - 1.0).abs() < 0.01,
        "the legacy clamp should sit at the clip point here"
    );
    assert_eq!(clip(raw(0.95)), 1.0, "legacy clips a sun-facing slope flat");

    // The shoulder keeps a real gradient above the old clip point and never
    // reaches 1.0, so a sun-facing slope is shaded instead of a white shape.
    let a = shoulder(raw(0.62));
    let b = shoulder(raw(0.80));
    let c = shoulder(raw(0.95));
    assert!(a < b && b < c, "shoulder must stay monotonic: {a} {b} {c}");
    assert!(c < 1.0, "shoulder must not clip: {c}");
    assert!(c > 0.90, "shoulder must preserve authored brightness: {c}");

    // A sweep proves the plateau is gone: many distinct lit values above the
    // old clip point instead of one shared maximum.
    let mut clip_values = std::collections::BTreeSet::new();
    let mut shoulder_values = std::collections::BTreeSet::new();
    for step in 0..=100 {
        let ndl = 0.615 + (step as f32 / 100.0) * 0.385;
        clip_values.insert((clip(raw(ndl)) * 1000.0).round() as i64);
        shoulder_values.insert((shoulder(raw(ndl)) * 1000.0).round() as i64);
    }
    assert_eq!(clip_values.len(), 1, "legacy plateau is a single value");
    assert!(
        shoulder_values.len() > 20,
        "shoulder must spread the former plateau over many values, got {}",
        shoulder_values.len()
    );

    // The terrain fragment shader uses the same curve as the mesh shader;
    // otherwise a unit standing on a lit slope would be darker than the ground.
    assert!(
        TERRAIN_SHADER.contains("fn shoulder(value: vec3<f32>)"),
        "terrain shader lost its shoulder()"
    );
    assert!(
        TERRAIN_SHADER.contains("return shoulder(camera.ambient.rgb + camera.sun.rgb * ndl);"),
        "terrain surface_light must apply the shoulder"
    );
}

fn spread(colours: &[[f32; 4]], channel: usize) -> f32 {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for colour in colours {
        lo = lo.min(colour[channel]);
        hi = hi.max(colour[channel]);
    }
    hi - lo
}

#[test]
fn art14_water_edge() {
    let water = WaterMaterial::default();

    // A deep edge is the reported defect: the corner ramp already equals the
    // deep colour, so the legacy t*2 blend gives one value across all 8 cells.
    let legacy: Vec<[f32; 4]> = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            water_edge_colour(&water, t, 100.0, 1.0, t * 1024.0, 0.0, true)
        })
        .collect();
    let fixed: Vec<[f32; 4]> = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            water_edge_colour(&water, t, 100.0, 1.0, t * 1024.0, 0.0, false)
        })
        .collect();

    assert!(
        spread(&legacy, 2) < 1e-6,
        "legacy deep edge must reproduce the flat blue band: {}",
        spread(&legacy, 2)
    );
    // Linear-light blue is ~0.034 for the default deep palette, so the
    // measurable non-flatness is ~10 % of that value, not 0.01 absolute.
    let fixed_spread = spread(&fixed, 2);
    let fixed_max = fixed.iter().map(|colour| colour[2]).fold(0.0f32, f32::max);
    assert!(
        fixed_spread > 0.01 * fixed_max,
        "shipped edge must be non-flat: {fixed_spread} of max {fixed_max}"
    );
    for colour in &fixed {
        for channel in 0..3 {
            assert!(
                colour[channel] >= 0.0 && colour[channel] <= 1.0,
                "edge colour left the authored range: {colour:?}"
            );
        }
    }

    // The authored fade is still honoured: the outer rim is fully transparent,
    // so the frame dissolves into the clear colour instead of ending on a line.
    let faded = water_edge_colour(&water, 1.0, 100.0, 0.0, 1024.0, 0.0, false);
    assert_eq!(faded[3], 0.0);

    // A shallow edge gets a genuine depth ramp across the whole frame: the
    // inner colour is the shallow palette, the outer one approaches deep.
    let inner = water_edge_colour(&water, 0.0, 12.0, 1.0, 0.0, 0.0, false);
    let outer = water_edge_colour(&water, 1.0, 12.0, 1.0, 1024.0, 0.0, false);
    assert!(
        (inner[2] - outer[2]).abs() > 0.005,
        "shallow edge must deepen outward: {} -> {}",
        inner[2],
        outer[2]
    );
    assert!(inner[2] > outer[2], "shallow blue must be lighter than deep blue");
}
