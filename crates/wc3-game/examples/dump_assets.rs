//! Offline asset dump: textures/models from the real archive, written as PPM.
use std::path::Path;
use wc3_game::Assets;

fn save_ppm(path: &str, width: u32, height: u32, rgba: &[u8]) {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&px[..3]);
    }
    std::fs::write(path, out).unwrap();
    println!("WROTE {path} {width}x{height}");
}

fn main() {
    let mpq = "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq";
    let map = "/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_map/Maps/Campaign/Human04.w3m";
    let assets = Assets::load(Some(Path::new(mpq)), Some(Path::new(map)));
    println!("SOURCE {} map={} textures={} atlas={}", assets.source, assets.map_name, assets.textures.len(), assets.terrain_atlas.is_some());
    for line in &assets.debug {
        println!("DEBUG {line}");
    }
    for (index, texture) in assets.textures.iter().enumerate() {
        let mut sum = [0u64; 4];
        for px in texture.rgba.chunks_exact(4) {
            for c in 0..4 { sum[c] += px[c] as u64; }
        }
        let n = (texture.width * texture.height).max(1) as u64;
        let amin = texture.rgba.chunks_exact(4).map(|p| p[3]).min().unwrap_or(0);
        let amax = texture.rgba.chunks_exact(4).map(|p| p[3]).max().unwrap_or(0);
        println!("TEX {index} {}x{} avg=({},{},{},{}) alpha={}..{}", texture.width, texture.height, sum[0]/n, sum[1]/n, sum[2]/n, sum[3]/n, amin, amax);
        std::fs::write(format!("analysis/tex-{index}.rgba"), &texture.rgba).unwrap();
        save_ppm(&format!("analysis/tex-{index}.ppm"), texture.width, texture.height, &texture.rgba);
    }
    if let Some(atlas) = &assets.terrain_atlas {
        save_ppm("analysis/terrain-atlas.ppm", atlas.width, atlas.height, &atlas.rgba);
    }
    for model in &assets.models {
        println!("MODEL {:?} fallback={} seqs={} idle={:?} walk={:?} attack={:?} height={:.1}", model.kind, model.fallback, model.sequences.len(), model.idle, model.walk, model.attack, model.height);
        if let Some(pos) = assets.models.iter().position(|m| m.kind == model.kind) {
            let alphas = assets.mesh_alpha(pos, None, 0);
            println!("ALPHA {:?} t0={:?}", model.kind, alphas);
            if let Some(stand) = model.sequences.iter().find(|sequence| sequence.name.eq_ignore_ascii_case("Stand")) {
                let stand_alphas = assets.mesh_alpha(pos, Some(stand), stand.start);
                println!("ALPHA {:?} stand {}..{}={:?}", model.kind, stand.start, stand.end, stand_alphas);
            }
        }
        for (si, seq) in model.sequences.iter().enumerate() {
            println!("  SEQ {si} {} {}..{}", seq.name, seq.start, seq.end);
        }
        if let Some(mdx) = &model.model {
            for animation in &mdx.geoset_animations {
                let keys = animation.alpha_track.as_ref().map(|track| {
                    track.keys.iter().map(|k| format!("{}={:.2}", k.time, k.value)).collect::<Vec<_>>().join(",")
                }).unwrap_or_else(|| format!("static={:.2}", animation.alpha));
                println!("  GEOA geo={} {}", animation.geoset_id, keys);
            }
        }
        for (index, mesh) in model.meshes.iter().enumerate() {
            let mut min = glam::Vec3::splat(f32::INFINITY);
            let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
            for p in &mesh.positions { min = min.min(*p); max = max.max(*p); }
            let bones_used = mesh.bones.iter().filter(|b| !b.is_empty()).count();
            let layers: Vec<String> = mesh
                .layers
                .iter()
                .map(|layer| format!("t{} b{} a{:.2}{}", layer.texture, layer.blend, layer.alpha, if layer.team_color { " team" } else { "" }))
                .collect();
            println!("  MESH {index} tex={} verts={} tris={} bones={} layers=[{}] bounds=({:.1},{:.1},{:.1})..({:.1},{:.1},{:.1})", mesh.texture, mesh.positions.len(), mesh.indices.len()/3, bones_used, layers.join("; "), min.x, min.y, min.z, max.x, max.y, max.z);
        }
    }
}
