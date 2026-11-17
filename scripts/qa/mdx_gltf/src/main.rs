//! Convert WC3 MDX models to glTF 2.0 for headless visual checks.
//!
//! Uses the canonical `wc3_formats::mdx` parser (no second MDX reader) and the
//! canonical `wc3_formats::blp` decoder (no second BLP reader). The output is a
//! self-contained glTF (JSON + .bin + PNG textures) that can be inspected with
//! the standard ecosystem tools: gltf-validator, gltf-transform, trimesh,
//! assimp.
//!
//! Scope is deliberately narrow — static geometry for visual verification:
//! per-geoset primitives with POSITION/NORMAL/TEXCOORD_0, per-geoset materials
//! referencing the model's first texture layer, and the model bounds. Animation,
//! skinning, particles and team colours are out of scope and reported as such.
//!
//! MDX is Z-up; glTF is Y-up, so positions/normals are converted as
//! `(x, y, z) -> (x, z, -y)`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wc3_formats::{blp, mdx};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut model: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut texture_root: Option<PathBuf> = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--model" => model = args.next().map(PathBuf::from),
            "--out" => out_dir = args.next().map(PathBuf::from),
            "--texture-root" => texture_root = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                println!(
                    "usage: mdx-gltf --model <model.mdx> --out <dir> [--texture-root <analysis/war3_full>]"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(model_path) = model else {
        eprintln!("missing --model");
        return ExitCode::from(2);
    };
    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("."));
    let texture_root = texture_root.unwrap_or_else(|| PathBuf::from("."));

    match convert(&model_path, &out_dir, &texture_root) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("MDX_GLTF_FAIL {error}");
            ExitCode::from(1)
        }
    }
}

fn convert(model_path: &Path, out_dir: &Path, texture_root: &Path) -> Result<String, String> {
    let bytes = fs::read(model_path).map_err(|e| format!("read {}: {e}", model_path.display()))?;
    let model = mdx::MdxModel::parse(&bytes).map_err(|e| format!("parse mdx: {e}"))?;
    fs::create_dir_all(out_dir.join("textures"))
        .map_err(|e| format!("create out dir: {e}"))?;

    // ---- geometry -------------------------------------------------------
    struct Prim {
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
        material: Option<usize>,
    }
    let mut prims: Vec<Prim> = Vec::new();
    let mut skipped_empty = 0usize;
    let mut vertex_total = 0usize;
    let mut tri_total = 0usize;
    let mut bounds = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);

    for (geoset_index, geoset) in model.geosets.iter().enumerate() {
        if geoset.faces.len() < 3 || geoset.vertices.is_empty() {
            skipped_empty += 1;
            continue;
        }
        let uv = geoset.uv_sets.first();
        let mut positions = Vec::with_capacity(geoset.vertices.len());
        let mut normals = Vec::with_capacity(geoset.vertices.len());
        let mut uvs = Vec::with_capacity(geoset.vertices.len());
        for (index, vertex) in geoset.vertices.iter().enumerate() {
            // Z-up -> Y-up
            let position = [vertex.x, vertex.z, -vertex.y];
            for axis in 0..3 {
                bounds.0[axis] = bounds.0[axis].min(position[axis]);
                bounds.1[axis] = bounds.1[axis].max(position[axis]);
            }
            positions.push(position);
            match geoset.normals.get(index) {
                Some(n) => normals.push([n.x, n.z, -n.y]),
                None => normals.push([0.0, 1.0, 0.0]),
            }
            match uv.and_then(|set| set.get(index)) {
                Some(t) => uvs.push([t.x, 1.0 - t.y]),
                None => uvs.push([0.0, 0.0]),
            }
        }
        let indices: Vec<u32> = geoset.faces.iter().map(|index| u32::from(*index)).collect();
        if indices.iter().any(|index| *index as usize >= positions.len()) {
            return Err(format!(
                "geoset {geoset_index}: face index outside {} vertices",
                positions.len()
            ));
        }
        vertex_total += positions.len();
        tri_total += indices.len() / 3;
        prims.push(Prim {
            positions,
            normals,
            uvs,
            indices,
            material: material_for(&model, geoset),
        });
    }
    if prims.is_empty() {
        return Err(format!(
            "no drawable geosets (empty={skipped_empty}, geosets={})",
            model.geosets.len()
        ));
    }

    // ---- textures -------------------------------------------------------
    // Resolve every texture referenced by a material layer to a PNG next to
    // the glTF. Replaceable ids (team colour, team glow) are not files: they
    // stay untextured and are reported in the summary instead.
    let mut texture_uri: BTreeMap<String, String> = BTreeMap::new();
    let mut texture_failed: Vec<String> = Vec::new();
    let mut material_texture: BTreeMap<usize, String> = BTreeMap::new();
    for prim in &prims {
        let Some(material_index) = prim.material else {
            continue;
        };
        let Some(path) = prim_texture_path(&model, material_index) else {
            continue;
        };
        if texture_uri.contains_key(&path) {
            material_texture.insert(material_index, texture_uri[&path].clone());
            continue;
        }
        match load_texture(texture_root, &path) {
            Ok(Some((rgba, width, height))) => {
                // glTF image URIs are URLs: keep the last two archive
                // components (textures can share a stem) and replace separators.
                let parts: Vec<String> = path
                    .replace('\\', "/")
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(|part| part.trim_end_matches(".blp").to_string())
                    .collect();
                let raw = parts.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("_");
                let stem: String = raw
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                    .collect();
                let stem = if stem.is_empty() { "texture".to_string() } else { stem };
                let uri = format!("textures/{stem}_{width}x{height}.png");
                fs::write(out_dir.join(&uri), encode_png(width, height, &rgba))
                    .map_err(|e| format!("write {uri}: {e}"))?;
                texture_uri.insert(path.clone(), uri.clone());
                material_texture.insert(material_index, uri);
            }
            Ok(None) => texture_failed.push(format!("{path}: not found under texture root")),
            Err(error) => texture_failed.push(format!("{path}: {error}")),
        }
    }

    // ---- glTF -----------------------------------------------------------
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views: Vec<String> = Vec::new();
    let mut accessors: Vec<String> = Vec::new();
    let mut gltf_primitives: Vec<String> = Vec::new();
    let mut materials_json: Vec<String> = Vec::new();
    let mut material_index_of: BTreeMap<usize, usize> = BTreeMap::new();

    for prim in &prims {
        let key = prim.material.unwrap_or(usize::MAX);
        let material_index = match material_index_of.get(&key) {
            Some(index) => *index,
            None => {
                let name = match prim.material {
                    Some(index) => format!("material_{index}"),
                    None => "default".to_string(),
                };
                let pbr = match prim.material.and_then(|index| material_texture.get(&index)) {
                    Some(uri) => {
                        let texture_index = texture_uri
                            .values()
                            .position(|candidate| candidate == uri)
                            .unwrap_or(0);
                        format!(
                            "{{\"baseColorTexture\":{{\"index\":{texture_index}}},\"metallicFactor\":0.0,\"roughnessFactor\":0.8}}"
                        )
                    }
                    None => {
                        "{\"baseColorFactor\":[0.72,0.72,0.72,1.0],\"metallicFactor\":0.0,\"roughnessFactor\":0.8}"
                            .to_string()
                    }
                };
                materials_json.push(format!(
                    "{{\"name\":\"{name}\",\"pbrMetallicRoughness\":{pbr},\"alphaMode\":\"MASK\",\"doubleSided\":true}}"
                ));
                let index = materials_json.len() - 1;
                material_index_of.insert(key, index);
                index
            }
        };
        let position = push_accessor_vec3(&mut bin, &mut buffer_views, &mut accessors, &prim.positions);
        let normal = push_accessor_vec3(&mut bin, &mut buffer_views, &mut accessors, &prim.normals);
        let indices = push_accessor_indices(&mut bin, &mut buffer_views, &mut accessors, &prim.indices);
        let textured = prim.material.and_then(|index| material_texture.get(&index)).is_some();
        let attributes = if textured {
            let uv = push_accessor_vec2(&mut bin, &mut buffer_views, &mut accessors, &prim.uvs);
            format!("{{\"POSITION\":{position},\"NORMAL\":{normal},\"TEXCOORD_0\":{uv}}}")
        } else {
            format!("{{\"POSITION\":{position},\"NORMAL\":{normal}}}")
        };
        gltf_primitives.push(format!(
            "{{\"attributes\":{attributes},\"indices\":{indices},\"material\":{material_index},\"mode\":4}}"
        ));
    }

    let mut unique_uris: Vec<String> = Vec::new();
    for uri in texture_uri.values() {
        if !unique_uris.iter().any(|existing| existing == uri) {
            unique_uris.push(uri.clone());
        }
    }
    let images_json: Vec<String> = unique_uris
        .iter()
        .map(|uri| format!("{{\"uri\":\"{uri}\"}}"))
        .collect();
    let textures_json: Vec<String> = (0..images_json.len())
        .map(|index| format!("{{\"source\":{index},\"sampler\":0}}"))
        .collect();

    let model_name = if model.name.trim().is_empty() {
        model_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".to_string())
    } else {
        model.name.trim().to_string()
    };
    let textures_block = if images_json.is_empty() {
        String::new()
    } else {
        format!(
            "\"textures\":[{}],\"images\":[{}],\"samplers\":[{{\"magFilter\":9729,\"minFilter\":9987,\"wrapS\":10497,\"wrapT\":10497}}],",
            textures_json.join(","),
            images_json.join(",")
        )
    };
    let gltf = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"h2-qa-mdx-gltf\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"nodes\":[{{\"mesh\":0,\"name\":\"{name}\"}}],\"meshes\":[{{\"name\":\"{name}\",\"primitives\":[{primitives}]}}],{textures_block}\"materials\":[{materials}],\"accessors\":[{accessors}],\"bufferViews\":[{views}],\"buffers\":[{{\"uri\":\"model.bin\",\"byteLength\":{len}}}]}}",
        name = escape(&model_name),
        primitives = gltf_primitives.join(","),
        textures_block = textures_block,
        materials = materials_json.join(","),
        accessors = accessors.join(","),
        views = buffer_views.join(","),
        len = bin.len(),
    );
    fs::write(out_dir.join("model.bin"), &bin).map_err(|e| format!("write model.bin: {e}"))?;
    fs::write(out_dir.join("model.gltf"), &gltf).map_err(|e| format!("write model.gltf: {e}"))?;

    let summary = format!(
        "MDX_GLTF_OK model=\"{}\" primitives={} vertices={} triangles={} materials={} textures={} texture_failures={} skipped_empty_geosets={} bounds_min=[{:.1},{:.1},{:.1}] bounds_max=[{:.1},{:.1},{:.1}] out={}",
        model_name,
        prims.len(),
        vertex_total,
        tri_total,
        materials_json.len(),
        texture_uri.len(),
        texture_failed.len(),
        skipped_empty,
        bounds.0[0], bounds.0[1], bounds.0[2],
        bounds.1[0], bounds.1[1], bounds.1[2],
        out_dir.display(),
    );
    for failure in &texture_failed {
        eprintln!("TEXTURE_SKIP {failure}");
    }
    Ok(summary)
}

fn material_for(model: &mdx::MdxModel, geoset: &mdx::Geoset) -> Option<usize> {
    let index = geoset.material_id as usize;
    (index < model.materials.len()).then_some(index)
}

fn prim_texture_path(model: &mdx::MdxModel, material_id: usize) -> Option<String> {
    let material = model.materials.get(material_id)?;
    let layer = material.layers.first()?;
    let texture = model.textures.get(layer.texture_id as usize)?;
    if texture.path.is_empty() || texture.replaceable_id != 0 {
        return None;
    }
    Some(texture.path.clone())
}

/// Resolve `Textures\Foo.blp` under `root` (case-insensitive, `\` or `/`).
fn load_texture(root: &Path, texture_path: &str) -> Result<Option<(Vec<u8>, u32, u32)>, String> {
    let normalized = texture_path.replace('\\', "/");
    let direct = root.join(&normalized);
    let candidate = if direct.is_file() {
        Some(direct)
    } else {
        let mut found = None;
        let mut stack = vec![root.to_path_buf()];
        let needle = normalized.to_ascii_lowercase();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(relative) = path.strip_prefix(root) {
                    if relative.to_string_lossy().replace('\\', "/").to_ascii_lowercase() == needle {
                        found = Some(path);
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };
    let Some(path) = candidate else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let image = blp::decode(&bytes).map_err(|e| format!("decode blp: {e}"))?;
    Ok(Some((image.rgba, image.width, image.height)))
}

// ---------------------------------------------------------------------------
// glTF accessors + bufferViews
// ---------------------------------------------------------------------------

fn push_view(bin: &mut Vec<u8>, views: &mut Vec<String>, data: &[u8], target: u32) -> usize {
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let offset = bin.len();
    bin.extend_from_slice(data);
    views.push(format!(
        "{{\"buffer\":0,\"byteOffset\":{offset},\"byteLength\":{},\"target\":{target}}}",
        data.len()
    ));
    views.len() - 1
}

fn push_accessor_vec3(
    bin: &mut Vec<u8>,
    views: &mut Vec<String>,
    accessors: &mut Vec<String>,
    values: &[[f32; 3]],
) -> usize {
    let mut data = Vec::with_capacity(values.len() * 12);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for value in values {
        for axis in 0..3 {
            min[axis] = min[axis].min(value[axis]);
            max[axis] = max[axis].max(value[axis]);
            data.extend_from_slice(&value[axis].to_le_bytes());
        }
    }
    let view = push_view(bin, views, &data, 34962);
    accessors.push(format!(
        "{{\"bufferView\":{view},\"componentType\":5126,\"count\":{},\"type\":\"VEC3\",\"min\":[{},{},{}],\"max\":[{},{},{}]}}",
        values.len(),
        num(min[0]), num(min[1]), num(min[2]),
        num(max[0]), num(max[1]), num(max[2]),
    ));
    accessors.len() - 1
}

fn push_accessor_vec2(
    bin: &mut Vec<u8>,
    views: &mut Vec<String>,
    accessors: &mut Vec<String>,
    values: &[[f32; 2]],
) -> usize {
    let mut data = Vec::with_capacity(values.len() * 8);
    for value in values {
        data.extend_from_slice(&value[0].to_le_bytes());
        data.extend_from_slice(&value[1].to_le_bytes());
    }
    let view = push_view(bin, views, &data, 34962);
    accessors.push(format!(
        "{{\"bufferView\":{view},\"componentType\":5126,\"count\":{},\"type\":\"VEC2\"}}",
        values.len()
    ));
    accessors.len() - 1
}

fn push_accessor_indices(
    bin: &mut Vec<u8>,
    views: &mut Vec<String>,
    accessors: &mut Vec<String>,
    indices: &[u32],
) -> usize {
    let mut data = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        data.extend_from_slice(&index.to_le_bytes());
    }
    let view = push_view(bin, views, &data, 34963);
    accessors.push(format!(
        "{{\"bufferView\":{view},\"componentType\":5125,\"count\":{},\"type\":\"SCALAR\"}}",
        indices.len()
    ));
    accessors.len() - 1
}

fn num(value: f32) -> String {
    // Display for f32 is the shortest round-trip representation, so the
    // declared min/max stays bit-exact against the binary buffer.
    if value.is_finite() {
        format!("{value}")
    } else {
        "0".to_string()
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

// ---------------------------------------------------------------------------
// Minimal PNG encoder (stored deflate) — no image dependency
// ---------------------------------------------------------------------------

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let expected = width as usize * height as usize * 4;
    let pixels = if rgba.len() >= expected {
        &rgba[..expected]
    } else {
        // pad a short buffer rather than panicking
        let mut padded = vec![0u8; expected];
        padded[..rgba.len().min(expected)].copy_from_slice(&rgba[..rgba.len().min(expected)]);
        return encode_png_owned(width, height, padded);
    };
    encode_png_owned(width, height, pixels.to_vec())
}

fn encode_png_owned(width: u32, height: u32, rgba: Vec<u8>) -> Vec<u8> {
    let mut raw = Vec::with_capacity(height as usize * (1 + width as usize * 4));
    for row in 0..height as usize {
        raw.push(0); // filter: none
        let start = row * width as usize * 4;
        raw.extend_from_slice(&rgba[start..start + width as usize * 4]);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut offset = 0usize;
    while offset < raw.len() {
        let chunk = (raw.len() - offset).min(65535);
        let last = offset + chunk == raw.len();
        zlib.push(if last { 1 } else { 0 });
        zlib.extend_from_slice(&(chunk as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + chunk]);
        offset += chunk;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
