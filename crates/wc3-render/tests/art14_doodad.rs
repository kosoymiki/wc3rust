//! Agent A art push — doodad grounding (art14).
//!
//! CPU-side tests of the two helpers that wire the doodad blob-shadow fan and
//! the per-layer contact band into the static batch. No GPU, no MPQ, no map:
//! the geometry is built here so the assertions are exact and repeatable.

use glam::Vec3;
use wc3_render::mesh::{
    static_contact_band, MeshRenderer, MeshVertex, PipelineKind, StaticDrawSpec,
};

fn vertex(x: f32, y: f32, z: f32) -> MeshVertex {
    MeshVertex::colored([x, y, z], Vec3::Y, [0.0, 0.0], [1.0, 1.0, 1.0, 1.0])
}

#[test]
fn art14_doodad_contact() {
    // Two trees in one merged batch, standing at different ground heights.
    // The doodad builder bakes the instance transform into `position`, so the
    // band has to come from the vertices the spec actually indexes.
    let vertices = vec![
        vertex(0.0, 10.0, 0.0),    // tree A base
        vertex(0.0, 130.0, 0.0),   // tree A top   -> 10..130, height 120
        vertex(500.0, 20.0, 0.0),  // tree B base
        vertex(500.0, 100.0, 0.0), // tree B top
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];

    let band = static_contact_band(&vertices, &indices, 0, 3);
    assert_eq!(band[3], 1.0, "contact term must be enabled for an object spec");
    assert!((band[0] - 10.0).abs() < 1e-6, "base_y must be the lowest indexed vertex");
    // 8 % of 120 world units = 9.6, inside the [6, 24] clamp.
    assert!((band[1] - 9.6).abs() < 1e-5, "band = 8 % of the geometry height");
    assert!((band[2] - 0.55).abs() < 1e-6, "base brightness = 0.55");

    // A second spec over tree B alone gets its own base (the merged builder
    // emits one spec per authored layer, each indexing only its own geometry).
    let band_b = static_contact_band(&vertices, &[2, 3, 2], 0, 3);
    assert!((band_b[0] - 20.0).abs() < 1e-6, "each layer keeps its own base_y");

    // Empty / out-of-range and flat ranges disable contact rather than producing
    // a divide-by-zero band in the shader.
    assert_eq!(static_contact_band(&vertices, &indices, 99, 3), [0.0; 4]);
    let flat = [vertex(0.0, 7.0, 0.0), vertex(1.0, 7.0, 0.0), vertex(0.0, 7.0, 1.0)];
    assert_eq!(
        static_contact_band(&flat, &[0, 1, 2], 0, 3),
        [0.0; 4],
        "a zero-height range has no contact band"
    );

    // Height clamp: a very short object still gets the 6-unit floor, a very
    // tall one never darkens more than 24 units of its silhouette.
    let short = [vertex(0.0, 0.0, 0.0), vertex(0.0, 2.0, 0.0), vertex(1.0, 0.0, 0.0)];
    assert!((static_contact_band(&short, &[0, 1, 2], 0, 3)[1] - 6.0).abs() < 1e-6);
    let tall = [vertex(0.0, 0.0, 0.0), vertex(0.0, 1000.0, 0.0), vertex(1.0, 0.0, 0.0)];
    assert!((static_contact_band(&tall, &[0, 1, 2], 0, 3)[1] - 24.0).abs() < 1e-6);
}

#[test]
fn art14_doodad_shadow_pipeline() {
    let spec = |group: u8, blend: u32, color_scale: f32| StaticDrawSpec {
        index_start: 0,
        index_count: 12,
        texture: 0,
        blend,
        group,
        tint: [1.0; 4],
        color_scale,
    };

    // The doodad blob fan is a decal: blend 2 for sort order, group 2 for the
    // pass, but it MUST reach fs_shadow. Before the fix it went through
    // fs_main with texture 0, so it was multiplied by an unrelated BLP and by
    // the surface light instead of being a flat dark blob.
    assert_eq!(
        MeshRenderer::static_pipeline(&spec(2, 2, 1.0)),
        PipelineKind::Shadow,
        "doodad ground shadow must use the untextured shadow pipeline"
    );

    // Authored material layers and the terrain batch keep their engine filter
    // mode — no regression in the rest of the static batch.
    assert_eq!(MeshRenderer::static_pipeline(&spec(0, 0, 2.0)), PipelineKind::Opaque);
    assert_eq!(MeshRenderer::static_pipeline(&spec(0, 1, 1.0)), PipelineKind::AlphaTest);
    assert_eq!(MeshRenderer::static_pipeline(&spec(1, 2, 1.0)), PipelineKind::Blend);
}
