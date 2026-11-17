//! MDX -> glTF 2.0 exporter for 3D visual verification.
//!
//! Usage: `mdx-gltf <model.mdx> <out-dir> [--root <extracted-tree>]`
//!
//! Writes `<out-dir>/<stem>.gltf`, `<out-dir>/<stem>.bin` and, when `--root`
//! resolves the MDX texture paths, `<out-dir>/tex_<n>.png`. Geometry follows the
//! renderer's Z-up -> Y-up basis change (`x, z, -y`) so the exported model is
//! directly comparable with the in-game scene.
//!
//! This is a verification exporter, not an art pipeline: it writes bind-pose
//! geometry (no skinning/animation) with the authored UVs and decoded textures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glam::{Vec2, Vec3};
use wc3_formats::blp;
use wc3_formats::mdx::MdxModel;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: mdx-gltf <model.mdx> <out-dir> [--root <tree>]");
        std::process::exit(2);
    }
    let input = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    let root = args
        .iter()
        .position(|arg| arg == "--root")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from);
    if let Err(error) = run(&input, &out_dir, root.as_deref()) {
        eprintln!("mdx-gltf: {error}");
        std::process::exit(1);
    }
}

fn run(input: &Path, out_dir: &Path, root: Option<&Path>) -> Result<(), String> {
    let bytes = std::fs::read(input).map_err(|error| format!("read {input:?}: {error}"))?;
    let model = MdxModel::parse(&bytes).map_err(|error| format!("parse: {error}"))?;
    std::fs::create_dir_all(out_dir).map_err(|error| format!("mkdir: {error}"))?;
    let stem = input
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".to_string());
    let resolver = root.map(TextureRoot::new);

    // ------------------------------------------------------------ textures
    let mut images: Vec<(String, u32, u32)> = Vec::new();
    let mut texture_files: Vec<Option<String>> = Vec::new();
    for (index, texture) in model.textures.iter().enumerate() {
        let path = texture.path.trim_end_matches('\0').replace('/', "\\");
        let resolved = resolver
            .as_ref()
            .and_then(|resolver| resolver.find(&path))
            .and_then(|file| std::fs::read(&file).ok())
            .and_then(|bytes| blp::decode(&bytes).ok());
        let mut uri = None;
        if let Some(image) = resolved {
            let name = format!("tex_{index}.png");
            write_png(&out_dir.join(&name), image.width, image.height, &image.rgba)?;
            images.push((name.clone(), image.width, image.height));
            uri = Some(name);
        }
        texture_files.push(uri);
        println!(
            "TEXTURE index={index} path={path} decoded={} uri={:?}",
            texture_files[index].is_some(),
            texture_files[index]
        );
    }

    // ------------------------------------------------------------ geometry
    let mut bin: Vec<u8> = Vec::new();
    let mut accessors: Vec<String> = Vec::new();
    let mut buffer_views: Vec<String> = Vec::new();
    let mut meshes: Vec<String> = Vec::new();
    let mut nodes: Vec<String> = Vec::new();
    let mut materials: Vec<String> = Vec::new();
    let mut material_for_texture: HashMap<usize, usize> = HashMap::new();
    let mut bbox = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));

    for (geoset_index, geoset) in model.geosets.iter().enumerate() {
        if geoset.vertices.is_empty() || geoset.faces.len() < 3 {
            continue;
        }
        let material = geoset.material_id as usize;
        let material_index = if let Some(&index) = material_for_texture.get(&material) {
            index
        } else {
            let texture = model
                .materials
                .get(material)
                .and_then(|material| material.layers.first())
                .map(|layer| layer.texture_id as usize)
                .filter(|texture| *texture < model.textures.len());
            let uri = texture
                .and_then(|texture| texture_files.get(texture))
                .and_then(|uri| uri.clone());
            let name = format!("material_{}", materials.len());
            let material_json = match &uri {
                Some(uri) => {
                    let image_index = images
                        .iter()
                        .position(|(name, _, _)| name == uri)
                        .unwrap_or(0);
                    format!(
                        "{{\"name\":\"{name}\",\"pbrMetallicRoughness\":{{\"baseColorTexture\":{{\"index\":{image_index}}},\"metallicFactor\":0.0,\"roughnessFactor\":0.9}},\"doubleSided\":true}}"
                    )
                }
                None => format!(
                    "{{\"name\":\"{name}\",\"pbrMetallicRoughness\":{{\"baseColorFactor\":[0.7,0.7,0.72,1.0],\"metallicFactor\":0.0,\"roughnessFactor\":0.9}},\"doubleSided\":true}}"
                ),
            };
            materials.push(material_json);
            let index = materials.len() - 1;
            material_for_texture.insert(material, index);
            index
        };

        // POSITION (Z-up -> Y-up)
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let position_offset = bin.len();
        for vertex in &geoset.vertices {
            let p = Vec3::new(vertex.x, vertex.z, -vertex.y);
            min = min.min(p);
            max = max.max(p);
            for value in p.to_array() {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        bbox.0 = bbox.0.min(min);
        bbox.1 = bbox.1.max(max);
        let position_view = push_view(&mut buffer_views, position_offset, geoset.vertices.len() * 12, 34962);
        let position_accessor = push_accessor(
            &mut accessors,
            position_view,
            5126,
            geoset.vertices.len(),
            "VEC3",
            Some([min.x, min.y, min.z]),
            Some([max.x, max.y, max.z]),
        );

        // NORMAL
        let normal_offset = bin.len();
        let normals = if geoset.normals.len() == geoset.vertices.len() {
            geoset
                .normals
                .iter()
                .map(|n| Vec3::new(n.x, n.z, -n.y).normalize_or_zero())
                .collect()
        } else {
            vec![Vec3::Y; geoset.vertices.len()]
        };
        for normal in &normals {
            for value in normal.to_array() {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        let normal_view = push_view(&mut buffer_views, normal_offset, normals.len() * 12, 34962);
        let normal_accessor = push_accessor(
            &mut accessors,
            normal_view,
            5126,
            normals.len(),
            "VEC3",
            None,
            None,
        );

        // TEXCOORD_0 (glTF V axis points down)
        let uvs: Vec<Vec2> = geoset
            .uv_sets
            .first()
            .cloned()
            .unwrap_or_else(|| vec![Vec2::ZERO; geoset.vertices.len()]);
        let uv_offset = bin.len();
        for uv in &uvs {
            for value in [uv.x, 1.0 - uv.y] {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        let uv_view = push_view(&mut buffer_views, uv_offset, uvs.len() * 8, 34962);
        let uv_accessor = push_accessor(&mut accessors, uv_view, 5126, uvs.len(), "VEC2", None, None);

        // INDICES
        let index_offset = bin.len();
        for index in &geoset.faces {
            bin.extend_from_slice(&(*index as u32).to_le_bytes());
        }
        let index_view = push_view(&mut buffer_views, index_offset, geoset.faces.len() * 4, 34963);
        let index_accessor = push_accessor(
            &mut accessors,
            index_view,
            5125,
            geoset.faces.len(),
            "SCALAR",
            None,
            None,
        );

        meshes.push(format!(
            "{{\"name\":\"geoset_{geoset_index}\",\"primitives\":[{{\"attributes\":{{\"POSITION\":{position_accessor},\"NORMAL\":{normal_accessor},\"TEXCOORD_0\":{uv_accessor}}},\"indices\":{index_accessor},\"material\":{material_index},\"mode\":4}}]}}"
        ));
        let mesh_index = meshes.len() - 1;
        nodes.push(format!(
            "{{\"name\":\"geoset_{geoset_index}\",\"mesh\":{mesh_index}}}"
        ));
        println!(
            "GEOSET index={geoset_index} vertices={} faces={} material={material_index} bbox={:?}..{:?}",
            geoset.vertices.len(),
            geoset.faces.len() / 3,
            min.to_array(),
            max.to_array()
        );
    }

    if meshes.is_empty() {
        return Err("model has no geosets (particle/emitter-only)".to_string());
    }

    // ------------------------------------------------------------ glTF files
    let bin_name = format!("{stem}.bin");
    std::fs::write(out_dir.join(&bin_name), &bin).map_err(|error| format!("write bin: {error}"))?;
    let images_json: Vec<String> = images
        .iter()
        .map(|(name, _, _)| format!("{{\"uri\":\"{name}\"}}"))
        .collect();
    let samplers = "[{\"magFilter\":9729,\"minFilter\":9987,\"wrapS\":10497,\"wrapT\":10497}]";
    let textures_json: Vec<String> = (0..images.len())
        .map(|index| format!("{{\"sampler\":0,\"source\":{index}}}"))
        .collect();
    let node_indices: Vec<String> = (0..nodes.len()).map(|index| index.to_string()).collect();
    let gltf = format!(
        "{{\n  \"asset\":{{\"version\":\"2.0\",\"generator\":\"mdx-gltf (WC3 H2)\"}},\n  \"scene\":0,\n  \"scenes\":[{{\"nodes\":[{}]}}],\n  \"nodes\":[{}],\n  \"meshes\":[{}],\n  \"materials\":[{}],\n  \"images\":[{}],\n  \"samplers\":{samplers},\n  \"textures\":[{}],\n  \"accessors\":[{}],\n  \"bufferViews\":[{}],\n  \"buffers\":[{{\"uri\":\"{bin_name}\",\"byteLength\":{}}}]\n}}\n",
        node_indices.join(","),
        nodes.join(","),
        meshes.join(","),
        materials.join(","),
        images_json.join(","),
        textures_json.join(","),
        accessors.join(","),
        buffer_views.join(","),
        bin.len()
    );
    let gltf_path = out_dir.join(format!("{stem}.gltf"));
    std::fs::write(&gltf_path, gltf).map_err(|error| format!("write gltf: {error}"))?;
    println!(
        "GLTF model={} geosets={} materials={} textures={} bin_bytes={} bbox={:?}..{:?} out={}",
        model.name,
        meshes.len(),
        materials.len(),
        images.len(),
        bin.len(),
        bbox.0.to_array(),
        bbox.1.to_array(),
        gltf_path.display()
    );
    Ok(())
}

fn push_view(views: &mut Vec<String>, offset: usize, length: usize, target: u32) -> usize {
    views.push(format!(
        "{{\"buffer\":0,\"byteOffset\":{offset},\"byteLength\":{length},\"target\":{target}}}"
    ));
    views.len() - 1
}

fn push_accessor(
    accessors: &mut Vec<String>,
    view: usize,
    component_type: u32,
    count: usize,
    kind: &str,
    min: Option<[f32; 3]>,
    max: Option<[f32; 3]>,
) -> usize {
    let bounds = match (min, max) {
        (Some(min), Some(max)) => format!(
            ",\"min\":[{},{},{}],\"max\":[{},{},{}]",
            min[0], min[1], min[2], max[0], max[1], max[2]
        ),
        _ => String::new(),
    };
    accessors.push(format!(
        "{{\"bufferView\":{view},\"componentType\":{component_type},\"count\":{count},\"type\":\"{kind}\"{bounds}}}"
    ));
    accessors.len() - 1
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| format!("create {path:?}: {error}"))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("png header: {error}"))?;
    writer
        .write_image_data(rgba)
        .map_err(|error| format!("png data: {error}"))?;
    Ok(())
}

/// Case-insensitive loose-tree resolver for MDX texture paths.
struct TextureRoot {
    files: HashMap<String, PathBuf>,
}

impl TextureRoot {
    fn new(root: &Path) -> Self {
        let mut files = HashMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => stack.push(path),
                    Ok(kind) if kind.is_file() => {
                        if let Ok(relative) = path.strip_prefix(root) {
                            let key = relative
                                .to_string_lossy()
                                .replace('/', "\\")
                                .to_ascii_lowercase();
                            files.insert(key, path);
                        }
                    }
                    _ => {}
                }
            }
        }
        Self { files }
    }

    fn find(&self, path: &str) -> Option<PathBuf> {
        let key = path.replace('/', "\\").to_ascii_lowercase();
        self.files.get(&key).cloned()
    }
}
