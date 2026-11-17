use std::path::Path;
fn main() {
    let mpq = std::env::args().nth(1).expect("mpq");
    let inner = std::env::args().nth(2).expect("inner path");
    let mut storm = wc3_re::storm::StormFs::new(); storm.add_archive("war3", Path::new(&mpq)).expect("add");
    let bytes = storm.read(&inner).expect("read");
    let model = wc3_re::mdx::parse(&bytes).expect("parse");
    println!("geosets={} geoset_animations={} sequences={} materials={}", model.geosets.len(), model.geoset_animations.len(), model.sequences.len(), model.materials.len());
    for (i, geo) in model.geosets.iter().enumerate() {
        let mat = model.materials.get(geo.material_id as usize);
        let tex = mat.and_then(|m| m.layers.first()).and_then(|l| model.textures.get(l.texture_id as usize)).map(|t| t.path.clone()).unwrap_or_default();
        println!("geoset {i} verts={} mat={} tex={tex}", geo.vertices.len(), geo.material_id);
    }
    for (i, anim) in model.geoset_animations.iter().enumerate() {
        println!("geoanim {i} geoset={} alpha_static={:?} alpha_keys={} color={}", anim.geoset_id, anim.alpha, anim.alpha_track.as_ref().map(|t| t.keys.len()).unwrap_or(0), anim.color);
    }
    for (i, seq) in model.sequences.iter().enumerate() {
        println!("seq {i} name={} start={} end={}", seq.name, seq.start, seq.end);
    }
}
