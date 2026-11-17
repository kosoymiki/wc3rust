fn main() {
    let path = std::env::args().nth(1).expect("model path");
    let bytes = std::fs::read(&path).expect("read");
    let model = wc3_formats::mdx::MdxModel::parse(&bytes).expect("parse");
    for (i, geo) in model.geosets.iter().enumerate() {
        let mat = model.materials.get(geo.material_id as usize);
        let layers: Vec<String> = mat
            .map(|m| {
                m.layers
                    .iter()
                    .map(|l| {
                        let tex = model
                            .textures
                            .get(l.texture_id as usize)
                            .map(|t| format!("{}:{}", t.replaceable_id, t.path))
                            .unwrap_or_else(|| "none".into());
                        format!("tex={tex}")
                    })
                    .collect()
            })
            .unwrap_or_default();
        let anim = model
            .geoset_animations
            .iter()
            .find(|a| a.geoset_id == i as u32);
        println!(
            "geoset {i} verts={} mat={} {layers:?} anim_alpha={:?} track_keys={:?}",
            geo.vertices.len(),
            geo.material_id,
            anim.map(|a| a.alpha),
            anim.and_then(|a| a.alpha_track.as_ref())
                .map(|t| t.keys.iter().map(|k| (k.time, k.value)).collect::<Vec<_>>())
        );
    }
    if let Some(seq) = model.sequences.iter().find(|s| s.name.eq_ignore_ascii_case("Stand")) {
        let mid=(seq.start+seq.end)/2;
        let hidden=vec![false; model.geosets.len()];
        println!("STAND_ALPHAS={:?}", wc3_anim::mesh_alphas(&model,&hidden,Some(seq),mid));
    }
}

#[allow(dead_code)]
fn show_stand(model: &wc3_formats::mdx::MdxModel) {}
