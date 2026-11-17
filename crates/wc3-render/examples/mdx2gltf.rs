//! MDX -> glTF 2.0 exporter for tool-side visual checks.
//!
//! The renderer itself does not need glTF, but review tooling does: `assimp`,
//! `gltf-transform inspect`, `trimesh` and browser viewers all consume glTF.
//! This example dumps each geoset as a glTF primitive (positions, normals,
//! texcoord 0, u32 indices) so a model can be inspected outside the game.
//!
//! ```text
//! cargo run -p wc3-render --release --example mdx2gltf -- Footman.mdx footman.gltf
//! ```
//!
//! Coordinates: WC3 is Z-up, glTF is Y-up; `(x, y, z) -> (x, z, -y)` is a pure
//! rotation, so triangle winding is preserved. Textures are BLP and are not
//! referenced; the exported materials are placeholders for shading only.

use std::fmt::Write as _;
use std::path::PathBuf;

use glam::Vec2;
use wc3_formats::mdx::MdxModel;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: mdx2gltf <input.mdx> <output.gltf>");
        std::process::exit(2);
    };
    let output = args.next().unwrap_or_else(|| {
        let mut path = PathBuf::from(&input);
        path.set_extension("gltf");
        path.to_string_lossy().into_owned()
    });
    let bytes = std::fs::read(&input).expect("read MDX");
    let model = MdxModel::parse(&bytes).expect("parse MDX");
    let bin_path = PathBuf::from(&output).with_extension("bin");
    let bin_uri = bin_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model.bin".into());
    let gltf = Exporter::new(&model).build(&bin_uri);
    std::fs::write(&bin_path, &gltf.bin).expect("write bin");
    std::fs::write(&output, gltf.json).expect("write gltf");
    println!(
        "MDX2GLTF input={input} output={output} bin={} geosets={} vertices={} triangles={} bytes={}",
        bin_path.display(),
        gltf.geosets,
        gltf.vertices,
        gltf.triangles,
        gltf.bin.len()
    );
}

struct Gltf {
    json: String,
    bin: Vec<u8>,
    geosets: usize,
    vertices: usize,
    triangles: usize,
}

struct Exporter<'a> {
    model: &'a MdxModel,
}

impl<'a> Exporter<'a> {
    fn new(model: &'a MdxModel) -> Self {
        Self { model }
    }

    fn build(&self, bin_uri: &str) -> Gltf {
        // Collect primitives first so accessor/bufferView indices are stable.
        struct Prim {
            positions: Vec<[f32; 3]>,
            normals: Vec<[f32; 3]>,
            uvs: Vec<[f32; 2]>,
            indices: Vec<u32>,
            material: usize,
        }
        let mut prims: Vec<Prim> = Vec::new();
        for geoset in &self.model.geosets {
            if geoset.vertices.is_empty() || geoset.faces.len() < 3 {
                continue;
            }
            let positions: Vec<[f32; 3]> = geoset
                .vertices
                .iter()
                .map(|v| [v.x, v.z, -v.y])
                .collect();
            let normals: Vec<[f32; 3]> = if geoset.normals.len() == geoset.vertices.len() {
                geoset.normals.iter().map(|n| [n.x, n.z, -n.y]).collect()
            } else {
                Vec::new()
            };
            let uvs: Vec<[f32; 2]> = geoset
                .uv_sets
                .first()
                .filter(|uv| uv.len() == geoset.vertices.len())
                .map(|uv| uv.iter().map(|uv: &Vec2| [uv.x, uv.y]).collect())
                .unwrap_or_default();
            let indices: Vec<u32> = geoset.faces.iter().map(|face| *face as u32).collect();
            if indices.iter().any(|index| *index as usize >= positions.len()) {
                continue; // corrupt face list: skip rather than write an invalid file
            }
            let material = geoset.material_id as usize;
            prims.push(Prim {
                positions,
                normals,
                uvs,
                indices,
                material,
            });
        }
        if prims.is_empty() {
            panic!("no drawable geosets in {}", self.model.name);
        }

        let mut bin: Vec<u8> = Vec::new();
        struct Accessor {
            view: usize,
            component: u32,
            count: usize,
            kind: &'static str,
            min: Option<[f32; 3]>,
            max: Option<[f32; 3]>,
        }
        let mut views: Vec<(usize, usize)> = Vec::new(); // (offset, length)
        let mut accessors: Vec<Accessor> = Vec::new();
        let mut push_view = |bin: &mut Vec<u8>, data: &[u8]| -> usize {
            while bin.len() % 4 != 0 {
                bin.push(0);
            }
            let offset = bin.len();
            bin.extend_from_slice(data);
            views.push((offset, data.len()));
            views.len() - 1
        };

        // Buffer for primitives: accessor indices per primitive.
        let mut prim_accessors: Vec<[usize; 4]> = Vec::new();
        for prim in &prims {
            let pos_bytes: &[u8] = bytemuck::cast_slice(&prim.positions);
            let pos_view = push_view(&mut bin, pos_bytes);
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for position in &prim.positions {
                for axis in 0..3 {
                    min[axis] = min[axis].min(position[axis]);
                    max[axis] = max[axis].max(position[axis]);
                }
            }
            let pos_acc = accessors.len();
            accessors.push(Accessor {
                view: pos_view,
                component: 5126,
                count: prim.positions.len(),
                kind: "VEC3",
                min: Some(min),
                max: Some(max),
            });

            let normal_acc = if prim.normals.is_empty() {
                usize::MAX
            } else {
                let bytes: &[u8] = bytemuck::cast_slice(&prim.normals);
                let view = push_view(&mut bin, bytes);
                let index = accessors.len();
                accessors.push(Accessor {
                    view,
                    component: 5126,
                    count: prim.normals.len(),
                    kind: "VEC3",
                    min: None,
                    max: None,
                });
                index
            };

            let uv_acc = if prim.uvs.is_empty() {
                usize::MAX
            } else {
                let bytes: &[u8] = bytemuck::cast_slice(&prim.uvs);
                let view = push_view(&mut bin, bytes);
                let index = accessors.len();
                accessors.push(Accessor {
                    view,
                    component: 5126,
                    count: prim.uvs.len(),
                    kind: "VEC2",
                    min: None,
                    max: None,
                });
                index
            };

            let index_bytes: &[u8] = bytemuck::cast_slice(&prim.indices);
            let index_view = push_view(&mut bin, index_bytes);
            let index_acc = accessors.len();
            accessors.push(Accessor {
                view: index_view,
                component: 5125,
                count: prim.indices.len(),
                kind: "SCALAR",
                min: None,
                max: None,
            });
            prim_accessors.push([pos_acc, normal_acc, uv_acc, index_acc]);
        }
        if bin.len() % 4 != 0 {
            bin.resize(bin.len() + (4 - bin.len() % 4), 0);
        }

        let texture_count = self.model.textures.len();
        let material_count = self.model.materials.len().max(
            prims.iter().map(|prim| prim.material + 1).max().unwrap_or(0),
        );

        let mut json = String::new();
        json.push_str("{\n  \"asset\": {\"version\": \"2.0\", \"generator\": \"wc3rust mdx2gltf\"},\n");
        let _ = writeln!(json, "  \"scene\": 0,");
        json.push_str("  \"scenes\": [{\"nodes\": [0]}],\n");
        let name = escape_json(&self.model.name);
        let _ = writeln!(
            json,
            "  \"nodes\": [{{\"mesh\": 0, \"name\": \"{name}\"}}],"
        );
        json.push_str("  \"meshes\": [{\"name\": \"model\", \"primitives\": [\n");
        for (index, accessors_for_prim) in prim_accessors.iter().enumerate() {
            let [pos, normal, uv, indices] = *accessors_for_prim;
            let mut attributes = format!("\"POSITION\": {pos}");
            if normal != usize::MAX {
                let _ = write!(attributes, ", \"NORMAL\": {normal}");
            }
            if uv != usize::MAX {
                let _ = write!(attributes, ", \"TEXCOORD_0\": {uv}");
            }
            let prim = &prims[index];
            json.push_str(&format!(
                "    {{\"attributes\": {{{attributes}}}, \"indices\": {indices}, \"material\": {}, \"mode\": 4}}{}\n",
                prim.material.min(material_count.saturating_sub(1)),
                if index + 1 == prim_accessors.len() { "" } else { "," }
            ));
        }
        json.push_str("  ]}],\n");

        json.push_str("  \"materials\": [\n");
        for index in 0..material_count {
            json.push_str("    {\"name\": \"material_");
            json.push_str(&index.to_string());
            json.push_str("\", \"doubleSided\": true, \"pbrMetallicRoughness\": {\"baseColorFactor\": [0.8, 0.8, 0.8, 1.0], \"metallicFactor\": 0.0, \"roughnessFactor\": 0.85}}");
            if index + 1 != material_count {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("  ],\n");

        json.push_str("  \"buffers\": [{\"uri\": \"");
        json.push_str(&escape_json(bin_uri));
        let _ = writeln!(json, "\", \"byteLength\": {}}}],", bin.len());

        json.push_str("  \"bufferViews\": [\n");
        for (index, (offset, length)) in views.iter().enumerate() {
            let _ = write!(
                json,
                "    {{\"buffer\": 0, \"byteOffset\": {offset}, \"byteLength\": {length}}}{}",
                if index + 1 == views.len() { "\n" } else { ",\n" }
            );
        }
        json.push_str("  ],\n");

        json.push_str("  \"accessors\": [\n");
        for (index, accessor) in accessors.iter().enumerate() {
            let _ = write!(
                json,
                "    {{\"bufferView\": {}, \"componentType\": {}, \"count\": {}, \"type\": \"{}\"",
                accessor.view, accessor.component, accessor.count, accessor.kind
            );
            if let (Some(min), Some(max)) = (accessor.min, accessor.max) {
                let _ = write!(
                    json,
                    ", \"min\": [{}, {}, {}], \"max\": [{}, {}, {}]",
                    min[0], min[1], min[2], max[0], max[1], max[2]
                );
            }
            let _ = write!(
                json,
                "}}{}",
                if index + 1 == accessors.len() { "\n" } else { ",\n" }
            );
        }
        json.push_str("  ],\n");
        let _ = writeln!(json, "  \"extras\": {{\"textures_in_mdx\": {texture_count}}}");
        json.push_str("}\n");

        Gltf {
            json,
            bin,
            geosets: prims.len(),
            vertices: prims.iter().map(|prim| prim.positions.len()).sum(),
            triangles: prims.iter().map(|prim| prim.indices.len() / 3).sum(),
        }
    }
}

fn escape_json(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
